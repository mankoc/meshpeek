//! The 3D view: owns the renderer and camera, turns mouse input into camera moves, and shows the rendered
//! image. The scene is only re-rendered when the camera, the size or the scene changed.

use eframe::egui;
use eframe::egui_wgpu::RenderState;
use glam::DVec3;
use meshpeek_render::{Camera, ObjectId, Renderer, wgpu};

/// Standard views: direction from the focal point to the camera, and view-up.
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
    pub const ALL: [(View, &'static str, egui::Key); 7] = [
        (View::Isometric, "Isometric", egui::Key::Num0),
        (View::Front, "Front", egui::Key::Num1),
        (View::Back, "Back", egui::Key::Num2),
        (View::Left, "Left", egui::Key::Num3),
        (View::Right, "Right", egui::Key::Num4),
        (View::Top, "Top", egui::Key::Num5),
        (View::Bottom, "Bottom", egui::Key::Num6),
    ];

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

pub struct Viewport {
    pub renderer: Renderer,
    pub camera: Camera,
    /// Object the camera is centred on and orbits (sticky across standard views), if any.
    pub focus: Option<ObjectId>,
    texture: Option<egui::TextureId>,
    rendered: Option<(Camera, [u32; 2])>,
}

impl Viewport {
    pub fn new(render_state: &RenderState) -> Self {
        Self {
            renderer: Renderer::new(&render_state.device, Renderer::best_sample_count(&render_state.adapter)),
            camera: Camera::default(),
            focus: None,
            texture: None,
            rendered: None,
        }
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

    pub fn center_on(&mut self, id: ObjectId) {
        self.focus = Some(id);
        self.frame_target();
    }

    pub fn show_view(&mut self, view: View) {
        let (direction, up) = view.direction();
        let distance = self.camera.distance();
        self.camera.view_from(direction, up, distance);
        self.frame_target();
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, render_state: &RenderState) {
        let rect = ui.available_rect_before_wrap();
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

    /// Orientation gizmo in the top-right corner: the axes turn with the camera; clicking an axis end looks
    /// along that axis (same as the standard views).
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
                ends.push((centre + offset, a.dot(forward), label, color, view, sign > 0.0));
            }
        }
        ends.sort_by(|a, b| b.1.total_cmp(&a.1)); // far ends first, so near ones are drawn on top

        let pointer = response.hover_pos();
        let hovered = pointer.and_then(|p| {
            ends.iter().rev().find(|e| e.0.distance(p) < 11.0).map(|e| e.4) // nearest-to-viewer end wins
        });
        let painter = ui.painter_at(area);
        painter.circle_filled(
            centre,
            RADIUS + 12.0,
            egui::Color32::from_black_alpha(if response.hovered() { 70 } else { 40 }),
        );
        for &(pos, _, label, color, view, positive) in &ends {
            let highlight = hovered == Some(view);
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
        if let Some(view) = hovered {
            let names = ["Isometric", "Front", "Back", "Left", "Right", "Top", "Bottom"];
            let name = View::ALL.iter().zip(names).find(|((v, _, _), _)| *v == view).map_or("", |(_, n)| n);
            response.clone().on_hover_text(format!("{name} view"));
            if response.clicked() {
                self.show_view(view);
                ui.ctx().request_repaint();
            }
        }
    }

    /// VTK trackball controls: left drag orbits, middle or Shift+left drag pans, right drag or wheel zooms.
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
                self.camera.orbit(-200.0 * dx / w, -200.0 * dy / h); // a full-width drag turns 200 degrees, like VTK
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
