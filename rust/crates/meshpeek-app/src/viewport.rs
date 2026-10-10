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
