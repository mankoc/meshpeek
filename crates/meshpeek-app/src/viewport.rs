//! The 3D view: owns the renderer and camera, turns mouse input into camera moves, and shows the rendered
//! image. The scene is only re-rendered when the camera, the size or the scene changed.

use eframe::egui;
use eframe::egui_wgpu::RenderState;
use glam::DVec3;
use meshpeek_render::{Camera, ObjectId, Renderer, wgpu};

use crate::settings::SpaceMouseSettings;
use crate::spacemouse::Motion;

/// Standard views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Isometric,
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
}

impl View {
    /// Every view with its menu label and keyboard key.
    pub const ALL: [(View, &'static str, egui::Key); 7] = [
        (View::Isometric, "Isometric", egui::Key::Num0),
        (View::Front, "Front", egui::Key::Num1),
        (View::Back, "Back", egui::Key::Num2),
        (View::Left, "Left", egui::Key::Num3),
        (View::Right, "Right", egui::Key::Num4),
        (View::Top, "Top", egui::Key::Num5),
        (View::Bottom, "Bottom", egui::Key::Num6),
    ];

    /// Direction from the focal point to the camera, and view-up.
    fn direction(self) -> (DVec3, DVec3) {
        match self {
            View::Isometric => (DVec3::new(1.0, 1.0, 1.0), DVec3::Z),
            View::Front => (-DVec3::Y, DVec3::Z),
            View::Back => (DVec3::Y, DVec3::Z),
            View::Left => (-DVec3::X, DVec3::Z),
            View::Right => (DVec3::X, DVec3::Z),
            View::Top => (DVec3::Z, DVec3::Y),
            View::Bottom => (-DVec3::Z, DVec3::Y),
        }
    }
}

/// The 3D view: renderer, camera and the texture egui shows.
pub struct Viewport {
    /// Draws the scene.
    pub renderer: Renderer,
    /// Current point of view.
    pub camera: Camera,
    /// Object the camera is centred on and orbits (sticky across standard views), if any.
    pub focus: Option<ObjectId>,
    /// The rendered image registered with egui.
    texture: Option<egui::TextureId>,
    /// Camera and size of the last render, to skip rendering when nothing changed.
    rendered: Option<(Camera, [u32; 2])>,
    /// Height of the view in points, for converting pan speeds.
    height: f64,
    /// Mouse orbit around the world's vertical axis (turntable) instead of the view's axes (trackball).
    pub turntable: bool,
    /// World axis all orbiting is limited to (chosen in the gizmo), if any.
    pub lock: Option<DVec3>,
    /// The view was turned in a way a turntable cannot (axis lock, roll): the mouse then orbits freely, so that the
    /// next drag does not jump back to a level horizon. A standard view ends this.
    tilted: bool,
}

/// Axes closer to the line of sight than this (length of their image on the screen) are turned by circling the pointer.
const FACING: f64 = 0.3;

/// The camera's right, up and viewing directions.
fn frame_of(camera: &Camera) -> (DVec3, DVec3, DVec3) {
    let (right, forward) = (camera.right(), camera.direction());
    (right, right.cross(forward), forward)
}

/// How far (in the drag's unit) dragging by `dx`, `dy` (down) turns the scene about `axis`, so that its near side follows
/// the pointer; `None` when the axis points at the viewer, where a straight drag has no direction.
fn turn_by_drag(camera: &Camera, axis: DVec3, dx: f64, dy: f64) -> Option<f64> {
    let (right, up, _) = frame_of(camera);
    let (x, y) = (axis.dot(right), axis.dot(up));
    let length = x.hypot(y);
    (length >= FACING).then(|| (dx * y + dy * x) / length)
}

impl Viewport {
    /// A view with the best MSAA the adapter supports and the default camera.
    pub fn new(render_state: &RenderState) -> Self {
        Self {
            renderer: Renderer::new(&render_state.device, Renderer::best_sample_count(&render_state.adapter)),
            camera: Camera::default(),
            focus: None,
            texture: None,
            rendered: None,
            height: 600.0,
            turntable: true,
            lock: None,
            tilted: false,
        }
    }

    /// Pan by pixels of the view (test hook for the snapshot actions).
    pub fn pan_pixels(&mut self, dx: f64, dy: f64) {
        self.camera.pan(dx, dy, self.height);
    }

    /// Frame the centred object if there is one, otherwise everything.
    fn frame_target(&mut self) {
        let bounds = match self.focus {
            Some(id) => self.renderer.object_bounds(id),
            None => self.renderer.scene_bounds(),
        };
        if let Some((low, high)) = bounds {
            self.camera.frame(low, high);
        }
    }

    /// Frame the whole scene and stop orbiting a centred object.
    pub fn fit_all(&mut self) {
        self.focus = None;
        self.frame_target();
    }

    /// Frame one object and orbit around it until another is centred or everything is framed.
    pub fn center_on(&mut self, id: ObjectId) {
        self.focus = Some(id);
        self.frame_target();
    }

    /// Look along a standard view, framing the centred object (or everything).
    pub fn show_view(&mut self, view: View) {
        let (direction, up) = view.direction();
        let distance = self.camera.distance();
        self.camera.view_from(direction, up, distance);
        self.tilted = false;
        self.frame_target();
    }

    /// Turn the scene about the locked axis by `degrees`.
    fn turn_locked(&mut self, axis: DVec3, degrees: f64) {
        self.camera.orbit_about(axis, -degrees);
        self.tilted = true;
    }

    /// Turn the scene parallel to the screen (positive: clockwise).
    pub fn roll(&mut self, degrees: f64) {
        if degrees != 0.0 {
            self.camera.roll(degrees);
            self.tilted = true;
        }
    }

    /// Keyboard navigation while keys are held: Ctrl+arrows orbit, Shift+arrows pan, +/- or Page Up/Down zoom,
    /// Q/E roll (plain arrows are not used here: left/right step through the files of a folder). Speeds are per second, so the frame rate does not matter. Returns true while moving.
    pub fn keyboard(&mut self, ctx: &egui::Context) -> bool {
        if ctx.wants_keyboard_input() {
            return false; // typing in a text field
        }
        use egui::Key;
        let (dt, shift, ctrl, alt, down) = ctx.input(|i| {
            let keys = [
                Key::ArrowLeft,
                Key::ArrowRight,
                Key::ArrowUp,
                Key::ArrowDown,
                Key::Plus,
                Key::Equals,
                Key::PageUp,
                Key::Minus,
                Key::PageDown,
                Key::Q,
                Key::E,
            ];
            (
                i.stable_dt.min(0.1) as f64,
                i.modifiers.shift,
                i.modifiers.command,
                i.modifiers.alt,
                keys.map(|k| i.key_down(k)),
            )
        });
        let [mut left, mut right, mut up, mut down_arrow, plus, equals, page_up, minus, page_down, q, e] = down;
        if !(ctrl || shift) {
            (left, right, up, down_arrow) = (false, false, false, false); // plain arrows browse files instead
        }
        if alt || ![left, right, up, down_arrow, plus, equals, page_up, minus, page_down, q, e].contains(&true) {
            return false;
        }
        let horizontal = (left as i32 - right as i32) as f64; // +1 = like dragging left
        let vertical = (up as i32 - down_arrow as i32) as f64; // +1 = like dragging up
        if shift {
            let speed = 0.6 * self.height * dt; // pixels: 60% of the view height per second
            self.camera.pan(-horizontal * speed, -vertical * speed, self.height);
        } else if let Some(axis) = self.lock {
            // towards the viewer: the arrows turn it on the screen instead
            let turn = turn_by_drag(&self.camera, axis, -horizontal, -vertical)
                .unwrap_or((horizontal + vertical) * -axis.dot(self.camera.direction()).signum());
            if turn != 0.0 {
                self.turn_locked(axis, 90.0 * turn * dt);
            }
        } else {
            self.camera.orbit(90.0 * horizontal * dt, 90.0 * vertical * dt);
        }
        let zoom = ((plus || equals || page_up) as i32 - (minus || page_down) as i32) as f64;
        self.camera.dolly(1.8f64.powf(zoom * dt));
        self.roll(90.0 * (q as i32 - e as i32) as f64 * dt);
        true
    }

    /// Apply space mouse motion for one frame of `dt` seconds. Returns true while the cap is deflected.
    pub fn space_mouse(&mut self, motion: Motion, settings: &SpaceMouseSettings, dt: f64) -> bool {
        // Full deflection is about 350 device units; ignore small values around the rest position.
        let scale = |v: f32| {
            let v = (v / 350.0).clamp(-1.0, 1.0) as f64;
            if v.abs() < 0.04 { 0.0 } else { v * settings.sensitivity as f64 }
        };
        let [tx, ty, tz] = motion.translation.map(scale);
        let [rx, ry, rz] = motion.rotation.map(scale);
        if [tx, ty, tz, rx, ry, rz].iter().all(|v| *v == 0.0) {
            return false;
        }
        let sign = |invert: bool| if invert { -1.0 } else { 1.0 };
        let (pan, zoom, rotate) =
            (sign(settings.invert_pan), sign(settings.invert_zoom), sign(settings.invert_rotation));
        // "Object" mode: the model follows the cap.
        let speed = self.height * dt; // pan: one view height per second at full deflection
        self.camera.pan(pan * tx * speed, -pan * ty * speed, self.height);
        self.camera.dolly(2.5f64.powf(zoom * tz * dt));
        if let Some(axis) = self.lock {
            // the part of the cap's rotation that is about the axis
            let (right, up, forward) = frame_of(&self.camera);
            let turn = rx * axis.dot(right) + ry * axis.dot(up) + rz * axis.dot(forward);
            self.turn_locked(axis, rotate * turn * 120.0 * dt);
        } else {
            self.camera.orbit(-rotate * ry * 120.0 * dt, -rotate * rx * 120.0 * dt);
            self.camera.roll(rotate * rz * 120.0 * dt);
        }
        true
    }

    /// Handle mouse input, re-render if needed and draw the image and the gizmo into the remaining space.
    pub fn ui(&mut self, ui: &mut egui::Ui, render_state: &RenderState) {
        let rect = ui.available_rect_before_wrap();
        self.height = rect.height().max(1.0) as f64;
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        self.handle_input(ui, &response, rect);

        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
        let stale = self.rendered != Some((self.camera, size)) || self.renderer.needs_render(size);
        if stale {
            let view = self.renderer.render(&render_state.device, &render_state.queue, size, &self.camera);
            let mut egui_renderer = render_state.renderer.write();
            match self.texture {
                None => {
                    self.texture = Some(egui_renderer.register_native_texture(
                        &render_state.device,
                        view,
                        wgpu::FilterMode::Linear,
                    ))
                }
                Some(id) => egui_renderer.update_egui_texture_from_wgpu_texture(
                    &render_state.device,
                    view,
                    wgpu::FilterMode::Linear,
                    id,
                ),
            }
            self.rendered = Some((self.camera, size));
        }
        if let Some(texture) = self.texture {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(texture, rect, uv, egui::Color32::WHITE);
        }
        self.gizmo(ui, rect);
    }

    /// Orientation gizmo in the top-right corner: the axes turn with the camera. Clicking an axis limits rotation to it
    /// (again: frees it), double-clicking an end looks along it (same as the standard views). The arrows below turn the
    /// scene parallel to the screen.
    fn gizmo(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        const RADIUS: f32 = 36.0;
        let centre = egui::pos2(rect.right() - RADIUS - 18.0, rect.top() + RADIUS + 18.0);
        let area = egui::Rect::from_center_size(centre, egui::Vec2::splat(2.0 * RADIUS + 26.0));
        let response = ui.interact(area, ui.id().with("gizmo"), egui::Sense::click());

        let forward = self.camera.direction();
        let right = forward.cross(self.camera.up).normalize_or(DVec3::X);
        let up = right.cross(forward);
        let axes = [
            (DVec3::X, "X", egui::Color32::from_rgb(230, 70, 70), View::Right, View::Left),
            (DVec3::Y, "Y", egui::Color32::from_rgb(90, 200, 90), View::Back, View::Front),
            (DVec3::Z, "Z", egui::Color32::from_rgb(90, 130, 255), View::Top, View::Bottom),
        ];
        // (screen position, depth away from the viewer, label, colour, view, positive end)
        let mut ends = Vec::new();
        for (axis, label, color, positive, negative) in axes {
            for (sign, view) in [(1.0, positive), (-1.0, negative)] {
                let a = axis * sign;
                let offset = egui::vec2(a.dot(right) as f32, -a.dot(up) as f32) * RADIUS;
                ends.push((centre + offset, a.dot(forward), label, color, view, sign > 0.0, axis));
            }
        }
        ends.sort_by(|a, b| b.1.total_cmp(&a.1)); // far ends first, so near ones are drawn on top

        let pointer = response.hover_pos();
        let hovered = pointer.and_then(|p| {
            ends.iter().rev().find(|e| e.0.distance(p) < 11.0).map(|e| (e.4, e.6, e.2)) // nearest-to-viewer end wins
        });
        let painter = ui.painter_at(area);
        painter.circle_filled(
            centre,
            RADIUS + 12.0,
            egui::Color32::from_black_alpha(if response.hovered() { 70 } else { 40 }),
        );
        for &(pos, _, label, color, view, positive, axis) in &ends {
            let highlight = hovered.is_some_and(|h| h.0 == view);
            let locked = self.lock == Some(axis);
            let color = if self.lock.is_some() && !locked { color.gamma_multiply(0.35) } else { color };
            if locked {
                painter.line_segment([centre, pos], egui::Stroke::new(4.0_f32, egui::Color32::WHITE));
                painter.circle_filled(pos, if positive { 11.0 } else { 9.0 }, egui::Color32::WHITE);
            }
            if positive {
                painter.line_segment([centre, pos], egui::Stroke::new(2.0_f32, color));
                painter.circle_filled(pos, if highlight { 10.0 } else { 8.5 }, color);
                painter.text(
                    pos,
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(11.0),
                    egui::Color32::BLACK,
                );
            } else {
                let fill = if highlight { color } else { color.gamma_multiply(0.45) };
                painter.circle_filled(pos, if highlight { 8.0 } else { 6.5 }, fill);
            }
        }
        if let Some((view, axis, label)) = hovered {
            let name = View::ALL.iter().find(|(v, _, _)| *v == view).map_or("", |(_, label, _)| label);
            let locked = self.lock == Some(axis);
            let click =
                if locked { "Click: rotate freely again".into() } else { format!("Click: rotate only about {label}") };
            response.clone().on_hover_text(format!("{click}\nDouble-click: {} view", name.to_lowercase()));
            // a double-click is also two clicks, which lock and free again
            if response.clicked() {
                self.lock = (!locked).then_some(axis);
            }
            if response.double_clicked() {
                self.show_view(view);
            }
            if response.clicked() {
                ui.ctx().request_repaint();
            }
        }
        self.roll_arrows(ui, centre + egui::vec2(0.0, RADIUS + 28.0));
    }

    /// The two arrows that turn the scene parallel to the screen: by 15 degrees a click, on and on while held.
    fn roll_arrows(&mut self, ui: &mut egui::Ui, centre: egui::Pos2) {
        for (x, icon, sign, name) in [(-18.0, "⟲", -1.0, "left"), (18.0, "⟳", 1.0, "right")] {
            let at = egui::Rect::from_center_size(centre + egui::vec2(x, 0.0), egui::Vec2::splat(28.0));
            let button = ui
                .interact(at, ui.id().with(("roll", name)), egui::Sense::click())
                .on_hover_text(format!("Turn {name} on the screen (hold to keep turning)"));
            let alpha = if button.hovered() { 90 } else { 40 };
            ui.painter().circle_filled(at.center(), 13.0, egui::Color32::from_black_alpha(alpha));
            let font = egui::FontId::proportional(18.0);
            ui.painter().text(at.center(), egui::Align2::CENTER_CENTER, icon, font, egui::Color32::WHITE);
            if button.clicked() {
                self.roll(15.0 * sign);
                ui.ctx().request_repaint();
            }
            if button.is_pointer_button_down_on() {
                let (held, dt) = ui.input(|i| {
                    (i.pointer.press_start_time().map_or(0.0, |start| i.time - start), i.stable_dt.min(0.1) as f64)
                });
                if held > 0.3 {
                    self.roll(90.0 * sign * dt);
                }
                ui.ctx().request_repaint();
            }
        }
    }

    /// Mouse controls: left drag orbits (turntable around the world Z axis), middle or Shift+left drag pans, right drag or wheel zooms.
    fn handle_input(&mut self, ui: &egui::Ui, response: &egui::Response, rect: egui::Rect) {
        let delta = response.drag_delta();
        let (w, h) = (rect.width().max(1.0) as f64, rect.height().max(1.0) as f64);
        let shift = ui.input(|i| i.modifiers.shift);
        if delta != egui::Vec2::ZERO {
            let (dx, dy) = (delta.x as f64, delta.y as f64);
            if response.dragged_by(egui::PointerButton::Middle)
                || (shift && response.dragged_by(egui::PointerButton::Primary))
            {
                self.camera.pan(dx, dy, h);
            } else if response.dragged_by(egui::PointerButton::Primary) {
                // a full-width drag turns 200 degrees, like VTK
                if let Some(axis) = self.lock {
                    let turn = turn_by_drag(&self.camera, axis, 200.0 * dx / w, 200.0 * dy / h);
                    let turn = turn.unwrap_or_else(|| {
                        // circle the pointer around the middle of the view
                        let Some(now) = response.interact_pointer_pos().map(|p| p - rect.center()) else { return 0.0 };
                        let before = now - delta;
                        let angle = f64::from(before.x * now.y - before.y * now.x).atan2(f64::from(before.dot(now)));
                        // clockwise on the screen is positive here; about an axis away from the viewer, too
                        angle.to_degrees() * axis.dot(self.camera.direction()).signum()
                    });
                    if turn != 0.0 {
                        self.turn_locked(axis, turn);
                    }
                } else if self.turntable && !self.tilted {
                    self.camera.orbit_turntable(-200.0 * dx / w, -200.0 * dy / h);
                } else {
                    self.camera.orbit(-200.0 * dx / w, -200.0 * dy / h);
                }
            } else if response.dragged_by(egui::PointerButton::Secondary) {
                self.camera.dolly(1.01f64.powf(-dy));
            }
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y) as f64;
            if scroll != 0.0 {
                self.camera.dolly(1.1f64.powf(scroll / 50.0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where a point of the scene is on the screen (right, up).
    fn seen(camera: &Camera, p: DVec3) -> (f64, f64) {
        let (right, up, _) = frame_of(camera);
        (p.dot(right), p.dot(up))
    }

    #[test]
    fn the_near_side_follows_a_drag_about_the_locked_axis() {
        for (view, axis, dx, dy) in [
            (View::Front, DVec3::Z, 5.0, 0.0),
            (View::Front, DVec3::X, 0.0, 5.0),
            (View::Right, DVec3::Z, -5.0, 0.0),
            (View::Top, DVec3::X, 0.0, -5.0),
            (View::Isometric, DVec3::Z, 5.0, 0.0),
        ] {
            let (direction, up) = view.direction();
            let mut camera = Camera::default();
            camera.view_from(direction, up, 100.0);
            let near = -camera.direction() * 10.0;
            let before = seen(&camera, near);
            let turn = turn_by_drag(&camera, axis, dx, dy).unwrap();
            camera.orbit_about(axis, -turn);
            let after = seen(&camera, near);
            let moved = (after.0 - before.0) * dx - (after.1 - before.1) * dy;
            assert!(moved > 0.0, "{view:?} {axis}: {before:?} {after:?}");
        }
    }

    #[test]
    fn a_locked_axis_keeps_its_direction() {
        let mut camera = Camera::default();
        let before = (seen(&camera, DVec3::X), DVec3::X.dot(camera.direction()), camera.distance());
        camera.orbit_about(DVec3::X, 77.0);
        let after = (seen(&camera, DVec3::X), DVec3::X.dot(camera.direction()), camera.distance());
        assert!((before.0.0 - after.0.0).abs() < 1e-9 && (before.0.1 - after.0.1).abs() < 1e-9);
        assert!((before.1 - after.1).abs() < 1e-9 && (before.2 - after.2).abs() < 1e-9);
    }

    #[test]
    fn a_positive_roll_turns_the_scene_clockwise() {
        let mut camera = Camera::default();
        camera.view_from(-DVec3::Y, DVec3::Z, 100.0);
        camera.roll(15.0);
        assert!(seen(&camera, DVec3::Z).0 > 0.0); // what was up leans to the right
    }

    #[test]
    fn an_axis_towards_the_viewer_has_no_straight_drag() {
        let mut camera = Camera::default();
        camera.view_from(DVec3::Z, DVec3::Y, 100.0);
        assert_eq!(turn_by_drag(&camera, DVec3::Z, 5.0, 0.0), None);
        // circling: turning +5 degrees about Z, which points at the viewer, is counter-clockwise on the screen
        let mut turned = camera;
        turned.orbit_about(DVec3::Z, -5.0);
        assert!(seen(&turned, DVec3::X).1 > 0.0);
    }
}
