//! The MeshPeek window: menus, toolbar, objects tree, 3D view, status bar and dialogs.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, RichText};
use eframe::egui_wgpu::RenderState;
use meshpeek_core::{ObjectKind, SUPPORTED_EXTENSIONS};
use meshpeek_render::{ObjectId, Style};

use crate::loading::{self, LoadResult};
use crate::settings::{MATERIALS, SUPPORT_DETAIL, Settings, material, rgb};
use crate::viewport::{View, Viewport};

/// One row of the objects tree.
struct Entry {
    id: ObjectId,
    name: String,
    kind: ObjectKind,
    parent: Option<usize>,
    visible: bool,
    source: PathBuf,
    triangles: usize,
}

pub struct MeshPeekApp {
    render_state: RenderState,
    viewport: Viewport,
    settings: Settings,
    /// Settings being edited in the Preferences window (previewed live), if it is open.
    draft: Option<Settings>,
    entries: Vec<Entry>,
    skipped: Vec<String>,
    loaded_paths: Vec<PathBuf>,
    pending: usize,
    /// Files were opened into an empty scene: keep framing until all of them have arrived.
    fit_until_loaded: bool,
    results: (Sender<LoadResult>, Receiver<LoadResult>),
    status: String,
    wireframe: bool,
    show_objects: bool,
    show_about: bool,
    adapter: String,
    /// `MESHPEEK_SNAPSHOT=<file.png>`: once loading has finished, save a screenshot and quit (for tests).
    snapshot: Option<(PathBuf, u32)>,
}

/// Extra indent of a model's supports under it in the objects tree.
const CHILD_INDENT: f32 = 16.0;

const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const CLEAR: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
const PREFERENCES: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma);

impl MeshPeekApp {
    pub fn new(cc: &eframe::CreationContext<'_>, files: Vec<PathBuf>) -> Self {
        let render_state = cc.wgpu_render_state.clone().expect("MeshPeek needs the wgpu renderer");
        let info = render_state.adapter.get_info();
        let adapter = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
        log::info!("rendering with {adapter}");
        let mut app = Self {
            viewport: Viewport::new(&render_state),
            render_state,
            settings: Settings::load(),
            draft: None,
            entries: Vec::new(),
            skipped: Vec::new(),
            loaded_paths: Vec::new(),
            pending: 0,
            fit_until_loaded: false,
            results: channel(),
            status: "Open or drop STL, OBJ, PLY or LYS files".into(),
            wireframe: false,
            show_objects: true,
            show_about: false,
            adapter,
            snapshot: std::env::var_os("MESHPEEK_SNAPSHOT").map(|p| (PathBuf::from(p), 0)),
        };
        app.open_files(&cc.egui_ctx, files);
        app
    }

    // --- loading -----------------------------------------------------------------------------------

    fn open_files(&mut self, ctx: &egui::Context, paths: impl IntoIterator<Item = PathBuf>) {
        if self.entries.is_empty() && self.pending == 0 {
            self.fit_until_loaded = true;
        }
        for path in paths {
            self.status = format!("Loading {}…", file_name(&path));
            self.pending += 1;
            let device = self.render_state.device.clone();
            loading::spawn(path, self.settings.load_options(), device, self.results.0.clone(), ctx.clone());
        }
    }

    fn receive_loads(&mut self) {
        while let Ok(LoadResult { path, result, seconds }) = self.results.1.try_recv() {
            self.pending -= 1;
            let file = match result {
                Ok(file) => file,
                Err(e) => {
                    self.status = format!("Could not open {}: {e}", file_name(&path));
                    continue;
                }
            };
            let first = self.entries.is_empty();
            let base = self.entries.len();
            let triangles: usize = file.objects.iter().map(|o| o.mesh.triangles).sum();
            for plate in &file.plates {
                self.viewport.renderer.add_plate(*plate);
            }
            for obj in file.objects {
                let style = self.style(obj.kind, &self.settings);
                let (kind, triangles) = (obj.kind, obj.mesh.triangles);
                let id = self.viewport.renderer.add_object(
                    &self.render_state.device,
                    obj.mesh,
                    obj.transform,
                    style,
                    obj.visible,
                );
                self.entries.push(Entry {
                    id,
                    name: obj.name,
                    kind,
                    parent: obj.parent.map(|p| base + p),
                    visible: obj.visible,
                    source: path.clone(),
                    triangles,
                });
            }
            self.skipped.extend(file.skipped.iter().cloned());
            let count = self.entries.len() - base;
            self.status = format!(
                "{}: {count} objects, {} triangles in {:.0} ms",
                file_name(&path),
                thousands(triangles),
                seconds * 1000.0
            );
            if !file.skipped.is_empty() {
                self.status += &format!(" — {} skipped", file.skipped.len());
            }
            self.loaded_paths.push(path.clone());
            self.settings.add_recent(&path);
            self.settings.save();
            if self.fit_until_loaded {
                if first {
                    self.viewport.show_view(View::Isometric);
                }
                self.viewport.fit_all();
            }
        }
        if self.pending == 0 {
            self.fit_until_loaded = false;
        }
    }

    fn clear(&mut self) {
        self.viewport.renderer.clear();
        self.viewport.focus = None;
        self.entries.clear();
        self.skipped.clear();
        self.loaded_paths.clear();
    }

    fn reload(&mut self, ctx: &egui::Context) {
        let paths = self.loaded_paths.clone();
        self.clear();
        self.open_files(ctx, paths);
    }

    fn open_dialog(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new().add_filter("3D files", SUPPORTED_EXTENSIONS).pick_files();
        self.open_files(ctx, picked.unwrap_or_default());
    }

    // --- appearance --------------------------------------------------------------------------------

    fn style(&self, kind: ObjectKind, settings: &Settings) -> Style {
        let (color, name) = match kind {
            ObjectKind::Support => (settings.support_color, &settings.support_material),
            ObjectKind::Model => (settings.model_color, &settings.model_material),
        };
        Style { color: rgb(color), material: material(name).unwrap_or(MATERIALS[1].1) }
    }

    fn apply_styles(&mut self, settings: &Settings) {
        for entry in &self.entries {
            let style = self.style(entry.kind, settings);
            self.viewport.renderer.set_style(&self.render_state.queue, entry.id, style);
        }
    }

    // --- UI ----------------------------------------------------------------------------------------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&OPEN)) {
            self.open_dialog(ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&CLEAR)) {
            self.clear();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&PREFERENCES)) {
            self.draft = Some(self.settings.clone());
        }
        if ctx.wants_keyboard_input() {
            return; // typing in a text field
        }
        let pressed = |key| ctx.input(|i| i.key_pressed(key) && i.modifiers.is_none());
        if pressed(Key::F) {
            self.viewport.fit_all();
        }
        if pressed(Key::W) {
            self.set_wireframe(!self.wireframe);
        }
        for (view, _, key) in View::ALL {
            if pressed(key) {
                self.viewport.show_view(view);
            }
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        let supported =
            dropped.into_iter().filter(|p| SUPPORTED_EXTENSIONS.contains(&meshpeek_core::io::extension(p).as_str()));
        let supported: Vec<PathBuf> = supported.collect();
        if !supported.is_empty() {
            self.open_files(ctx, supported);
        }
    }

    fn set_wireframe(&mut self, on: bool) {
        self.wireframe = on && self.viewport.renderer.supports_wireframe();
        self.viewport.renderer.set_wireframe(self.wireframe);
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.add(egui::Button::new("Open…").shortcut_text(ctx.format_shortcut(&OPEN))).clicked() {
                    self.open_dialog(&ctx);
                }
                ui.menu_button("Open Recent", |ui| self.recent_menu(ui, &ctx));
                if ui.add(egui::Button::new("Clear scene").shortcut_text(ctx.format_shortcut(&CLEAR))).clicked() {
                    self.clear();
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("View", |ui| {
                if ui.add(egui::Button::new("Fit all").shortcut_text("F")).clicked() {
                    self.viewport.fit_all();
                }
                for (view, label, key) in View::ALL {
                    if ui.add(egui::Button::new(label).shortcut_text(key.name())).clicked() {
                        self.viewport.show_view(view);
                    }
                }
                ui.separator();
                let mut wireframe = self.wireframe;
                let supported = self.viewport.renderer.supports_wireframe();
                let toggle = ui.add_enabled(supported, egui::Checkbox::new(&mut wireframe, "Wireframe (W)"));
                if toggle.changed() {
                    self.set_wireframe(wireframe);
                }
                ui.checkbox(&mut self.show_objects, "Objects panel");
            });
            ui.menu_button("Settings", |ui| {
                if ui.add(egui::Button::new("Preferences…").shortcut_text(ctx.format_shortcut(&PREFERENCES))).clicked()
                {
                    self.draft = Some(self.settings.clone());
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About MeshPeek").clicked() {
                    self.show_about = true;
                }
            });
        });
    }

    fn recent_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.settings.recent_files.is_empty() {
            ui.add_enabled(false, egui::Button::new("No recent files"));
            return;
        }
        let mut open = None;
        for (number, path) in self.settings.recent_files.iter().enumerate() {
            let folder = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
            let label = format!("{}  {}  —  {folder}", (number + 1) % 10, file_name(path));
            let button =
                ui.add_enabled(path.exists(), egui::Button::new(label)).on_hover_text(path.display().to_string());
            if button.clicked() {
                open = Some(path.clone());
            }
        }
        ui.separator();
        if ui.button("Clear list").clicked() {
            self.settings.recent_files.clear();
            self.settings.save();
        }
        if let Some(path) = open {
            self.open_files(ctx, [path]);
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("📂 Open").on_hover_text("Open files (Ctrl+O)").clicked() {
                self.open_dialog(&ui.ctx().clone());
            }
            if ui.button("🗑 Clear scene").on_hover_text("Remove everything (Ctrl+W)").clicked() {
                self.clear();
            }
        });
    }

    fn objects_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for index in 0..self.entries.len() {
                if self.entries[index].parent.is_some() {
                    continue;
                }
                let children: Vec<usize> =
                    (0..self.entries.len()).filter(|&c| self.entries[c].parent == Some(index)).collect();
                if children.is_empty() {
                    self.object_row(ui, index, 0.0);
                } else {
                    let id = ui.make_persistent_id(("object", self.entries[index].id));
                    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                        .show_header(ui, |ui| self.object_row(ui, index, 0.0))
                        .body(|ui| children.iter().for_each(|&c| self.object_row(ui, c, CHILD_INDENT)));
                }
            }
            for reason in &self.skipped {
                ui.label(RichText::new(format!("⚠ {reason}")).color(Color32::from_rgb(0xc0, 0x8a, 0x3e)));
            }
        });
    }

    fn object_row(&mut self, ui: &mut egui::Ui, index: usize, indent: f32) {
        ui.horizontal(|ui| {
            ui.add_space(indent);
            let entry = &mut self.entries[index];
            if ui.checkbox(&mut entry.visible, "").changed() {
                let (id, visible) = (entry.id, entry.visible);
                self.viewport.renderer.set_visible(id, visible);
                // A model's checkbox also shows/hides what belongs to it (its supports).
                for child in self.entries.iter_mut().filter(|e| e.parent == Some(index)) {
                    child.visible = visible;
                    self.viewport.renderer.set_visible(child.id, visible);
                }
            }
            let entry = &self.entries[index];
            let tooltip = format!("{}\n{} triangles", entry.source.display(), thousands(entry.triangles));
            let label = ui.add(egui::Label::new(&entry.name).sense(egui::Sense::click())).on_hover_text(tooltip);
            let id = entry.id;
            label.context_menu(|ui| {
                if ui.button("Center on object").clicked() {
                    self.viewport.center_on(id);
                    ui.close();
                }
            });
            if self.viewport.focus == Some(id) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let button = ui.add(egui::Button::new("🎯").frame(false));
                    if button.on_hover_text("Camera is centred on this object. Click to stop centring.").clicked() {
                        self.viewport.fit_all();
                    }
                });
            }
        });
    }

    fn preferences_window(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.draft.take() else { return };
        let mut decision = None; // Some(true) = OK, Some(false) = Cancel
        let mut open = true;
        egui::Window::new("Preferences").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
            egui::Grid::new("preferences").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                let combo = |ui: &mut egui::Ui, id: &str, value: &mut String, options: &[&str]| {
                    egui::ComboBox::from_id_salt(id).selected_text(value.as_str()).show_ui(ui, |ui| {
                        for option in options {
                            ui.selectable_value(value, option.to_string(), *option);
                        }
                    });
                };
                let materials: Vec<&str> = MATERIALS.iter().map(|(n, _)| *n).collect();
                ui.label("Model material:");
                combo(ui, "model material", &mut draft.model_material, &materials);
                ui.end_row();
                ui.label("Model colour:");
                ui.color_edit_button_srgb(&mut draft.model_color).on_hover_text("Used when the file has no colours");
                ui.end_row();
                ui.label("Support material:");
                combo(ui, "support material", &mut draft.support_material, &materials);
                ui.end_row();
                ui.label("Support colour:");
                ui.color_edit_button_srgb(&mut draft.support_color);
                ui.end_row();
                ui.label("Support detail:");
                let details: Vec<&str> = SUPPORT_DETAIL.iter().map(|(n, _)| *n).collect();
                combo(ui, "support detail", &mut draft.support_detail, &details);
                ui.end_row();
                ui.label("OBJ files:");
                ui.checkbox(&mut draft.obj_y_up, "Y is up (Blender, Maya, ZBrush)")
                    .on_hover_text("Rotate OBJ files so their Y axis points up. Applied by reloading open files.");
                ui.end_row();
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    decision = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    decision = Some(false);
                }
            });
        });
        if !open {
            decision = Some(false);
        }
        match decision {
            None => {
                self.apply_styles(&draft); // live preview
                self.draft = Some(draft);
            }
            Some(false) => self.apply_styles(&self.settings.clone()),
            Some(true) => {
                let reload = draft.load_options() != self.settings.load_options();
                self.apply_styles(&draft);
                self.settings = draft;
                self.settings.save();
                if reload && !self.loaded_paths.is_empty() {
                    self.reload(ctx);
                }
            }
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("About MeshPeek").collapsible(false).resizable(false).open(&mut self.show_about).show(
            ctx,
            |ui| {
                ui.label(format!("MeshPeek {}", env!("CARGO_PKG_VERSION")));
                ui.label("Fast viewer for STL, OBJ, PLY and Lychee .lys files.");
                ui.separator();
                ui.label(format!("Rendering with: {}", self.adapter));
                ui.label(format!("Memory in use: {:.0} MB", memory_mb()));
            },
        );
    }
}

impl eframe::App for MeshPeekApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive_loads();
        self.shortcuts(ctx);

        egui::TopBottomPanel::top("menu").show(ctx, |ui| self.menu_bar(ui));
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| self.toolbar(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.pending > 0 {
                    ui.spinner();
                }
                ui.label(&self.status);
            });
        });
        if self.show_objects {
            egui::SidePanel::left("objects").resizable(true).default_width(230.0).show(ctx, |ui| {
                ui.heading("Objects");
                self.objects_panel(ui);
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let render_state = self.render_state.clone();
            self.viewport.ui(ui, &render_state);
        });
        self.preferences_window(ctx);
        self.about_window(ctx);
        self.snapshot(ctx);
    }
}

impl MeshPeekApp {
    /// Test hook: after loading finished and a few frames were drawn, capture the window to PNG and quit.
    fn snapshot(&mut self, ctx: &egui::Context) {
        let Some((path, frames)) = &mut self.snapshot else { return };
        let saved = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = saved {
            let [w, h] = image.size;
            let pixels: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Err(e) = image::save_buffer(&*path, &pixels, w as u32, h as u32, image::ColorType::Rgba8) {
                log::error!("snapshot failed: {e}");
            }
            println!(
                "snapshot: {} | {} | adapter: {} | memory: {:.0} MB",
                path.display(),
                self.status,
                self.adapter,
                memory_mb()
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.pending == 0 {
            *frames += 1;
            if *frames == 5 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
        ctx.request_repaint();
    }
}

fn memory_mb() -> f64 {
    memory_stats::memory_stats().map_or(0.0, |m| m.physical_mem as f64 / (1024.0 * 1024.0))
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn thousands_separators() {
        assert_eq!(super::thousands(1_986_598), "1,986,598");
        assert_eq!(super::thousands(999), "999");
    }
}
