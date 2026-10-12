//! The MeshPeek window: menus, toolbar, objects tree, 3D view, status bar and dialogs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, RichText, Stroke};
use eframe::egui_wgpu::RenderState;
use meshpeek_core::{ObjectKind, SUPPORTED_EXTENSIONS};
use meshpeek_render::{ObjectId, Palette, Style};

use crate::help;
use crate::loading::{self, LoadResult};
use crate::settings::{MATERIALS, ORBIT_STYLES, SUPPORT_DETAIL, Settings, THEMES, material, rgb};
use crate::sources::{self, Node};
use crate::spacemouse::{self, Motion};
use crate::viewport::{View, Viewport};

/// License of MeshPeek itself.
const LICENSE_NAME: &str = "CC BY-NC-SA 4.0";
const LICENSE_URL: &str = "https://creativecommons.org/licenses/by-nc-sa/4.0/";
/// Notices of the open-source libraries built into MeshPeek; shipped next to the executable.
const THIRD_PARTY_FILE: &str = "THIRD-PARTY-LICENSES.html";

/// One row of the objects tree.
struct Entry {
    id: ObjectId,
    name: String,
    kind: ObjectKind,
    parent: Option<usize>,
    visible: bool,
    source: PathBuf,
    triangles: usize,
    info: Vec<(String, String)>,
}

/// Colours handed out to the models in turn by "Colour each model".
const MODEL_COLORS: [[u8; 3]; 8] = [
    [0x4e, 0x9a, 0xe6],
    [0xe6, 0x6a, 0x4e],
    [0x5c, 0xc2, 0x7a],
    [0xe6, 0xc2, 0x4e],
    [0xb0, 0x6a, 0xe6],
    [0x4e, 0xd6, 0xd6],
    [0xe6, 0x6a, 0xb0],
    [0xa0, 0xa8, 0xb4],
];

#[derive(Clone, Copy, PartialEq)]
enum PanelTab {
    Objects,
    Files,
}

/// Depth limit when listing a folder in the Files tab.
const TREE_DEPTH: usize = 6;

/// Coloured toolbar icons (PNG files in assets/toolbar, made by make_toolbar_icons.py).
struct ToolIcons {
    open: egui::TextureHandle,
    folder: egui::TextureHandle,
    clear: egui::TextureHandle,
    objects: egui::TextureHandle,
    eye: egui::TextureHandle,
    colours: egui::TextureHandle,
    /// The application icon, shown large in the About window.
    app: egui::TextureHandle,
}

impl ToolIcons {
    fn load(ctx: &egui::Context) -> Self {
        let texture = |name: &str, png: &[u8]| {
            let image = image::load_from_memory(png).expect("toolbar icon").to_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            let image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            ctx.load_texture(name, image, egui::TextureOptions::LINEAR)
        };
        Self {
            open: texture("open", include_bytes!("../assets/toolbar/open.png")),
            folder: texture("folder", include_bytes!("../assets/toolbar/folder.png")),
            clear: texture("clear", include_bytes!("../assets/toolbar/clear.png")),
            objects: texture("objects", include_bytes!("../assets/toolbar/objects.png")),
            eye: texture("eye", include_bytes!("../assets/toolbar/eye.png")),
            colours: texture("colours", include_bytes!("../assets/toolbar/colours.png")),
            app: texture("app", include_bytes!("../assets/icon.png")),
        }
    }
}

pub struct MeshPeekApp {
    tool_icons: ToolIcons,
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
    show_toolbar: bool,
    panel_tab: PanelTab,
    /// Folder listed in the Files tab, with its tree and the files ticked in it.
    browse_root: Option<PathBuf>,
    tree: Option<Node>,
    /// Model entries of the archive listed in the Files tab (sorted), if it lists one.
    archive_models: Option<(PathBuf, Vec<String>)>,
    selected: BTreeSet<PathBuf>,
    /// Give every model its own colour instead of the preferences colour.
    color_models: bool,
    /// Entry whose information window is open.
    info_for: Option<usize>,
    show_about: bool,
    show_controls: bool,
    show_guide: bool,
    adapter: String,
    space_mouse: spacemouse::Shared,
    /// Connected space mouse, for the About window.
    space_device: Option<String>,
    /// `MESHPEEK_SNAPSHOT=<file.png>`: once loading has finished, save a screenshot and quit (for tests).
    snapshot: Option<(PathBuf, u32)>,
}

/// Extra indent of a model's supports under it in the objects tree.
const CHILD_INDENT: f32 = 16.0;

const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const CLEAR: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
const PREFERENCES: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma);
const CONTROLS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F1);

impl MeshPeekApp {
    pub fn new(cc: &eframe::CreationContext<'_>, files: Vec<PathBuf>) -> Self {
        let render_state = cc.wgpu_render_state.clone().expect("MeshPeek needs the wgpu renderer");
        let info = render_state.adapter.get_info();
        let adapter = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
        log::info!("rendering with {adapter}");
        let mut app = Self {
            tool_icons: ToolIcons::load(&cc.egui_ctx),
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
            show_toolbar: true,
            panel_tab: PanelTab::Objects,
            browse_root: None,
            tree: None,
            archive_models: None,
            selected: BTreeSet::new(),
            color_models: false,
            info_for: None,
            show_about: false,
            show_controls: false,
            show_guide: false,
            adapter,
            space_mouse: spacemouse::start(cc.egui_ctx.clone()),
            space_device: None,
            snapshot: std::env::var_os("MESHPEEK_SNAPSHOT").map(|p| (PathBuf::from(p), 0)),
        };
        app.open_files(&cc.egui_ctx, files);
        app
    }

    // --- loading -----------------------------------------------------------------------------------

    /// Open files and folders (all models inside). An archive lists its models in the Files tab like a folder
    /// and opens only the first one; the others open from there (or with the arrow keys).
    fn open_files(&mut self, ctx: &egui::Context, paths: impl IntoIterator<Item = PathBuf>) {
        let mut paths: Vec<PathBuf> = paths.into_iter().collect();
        for path in &mut paths {
            if path.is_file() && sources::is_archive(path) {
                let archive = path.clone();
                match self.set_browse_root(Some(archive.clone())).ok().and_then(|_| self.first_model(&archive)) {
                    Some(first) => *path = first,
                    None => self.status = format!("Could not open {}: no model files inside", archive.display()),
                }
            }
        }
        if let Some(first) = paths.first() {
            let inside = self.browse_root.as_ref().is_some_and(|root| first.starts_with(root));
            if !inside {
                let root = if first.is_dir() { Some(first.clone()) } else { first.parent().map(Path::to_path_buf) };
                let _ = self.set_browse_root(root);
            }
        }
        for path in &paths {
            if !self.loaded_paths.contains(path) {
                self.loaded_paths.push(path.clone()); // what Reload opens again
            }
        }
        let mut problems = Vec::new();
        let paths = sources::expand(paths, &mut problems);
        if let Some(problem) = problems.first() {
            self.status = format!("Could not open {problem}");
        }
        if self.entries.is_empty() && self.pending == 0 {
            self.fit_until_loaded = true;
        }
        for source in paths {
            self.status = format!("Loading {}…", file_name(source.path()));
            self.pending += 1;
            let device = self.render_state.device.clone();
            loading::spawn(source, self.settings.load_options(), device, self.results.0.clone(), ctx.clone());
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
                let style = self.style(obj.kind, self.model_count(), &self.settings);
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
                    info: obj.info,
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
            if path.exists() {
                self.settings.add_recent(&path); // not files read from inside an archive
                self.settings.save();
            }
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
        self.info_for = None;
        self.skipped.clear();
        self.loaded_paths.clear();
    }

    fn reload(&mut self, ctx: &egui::Context) {
        let paths = self.loaded_paths.clone();
        self.clear();
        self.open_files(ctx, paths);
    }

    /// Open the previous (`-1`) or next (`1`) model file of the current file's folder, alone in the scene.
    fn step_file(&mut self, ctx: &egui::Context, step: isize) {
        if self.pending > 0 {
            return; // still loading: the old scene would reappear
        }
        let Some(current) = self.loaded_paths.last().cloned() else { return };
        let key = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase(); // whole path: names can repeat in sub-folders
        let mut files: Vec<PathBuf> = if let Some((archive, _)) = sources::archive_of(&current) {
            // a model inside an archive: its neighbours are the archive's other models
            let entries = match &self.archive_models {
                Some((listed, entries)) if *listed == archive => entries.clone(),
                _ => sources::list_models(&archive).unwrap_or_default(),
            };
            entries.iter().map(|e| archive.join(e)).collect()
        } else if current.is_file() && sources::is_model(&current) {
            let Some(folder) = current.parent() else { return };
            let mut files: Vec<PathBuf> = std::fs::read_dir(folder)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && sources::is_model(p))
                .collect();
            files.sort_by_key(|p| key(p));
            files
        } else {
            return;
        };
        let Some(index) = files.iter().position(|p| key(p) == key(&current)) else { return };
        if files.len() < 2 {
            return;
        }
        let next = (index as isize + step).rem_euclid(files.len() as isize) as usize;
        let path = files.swap_remove(next);
        self.clear();
        self.open_files(ctx, [path]);
    }

    fn open_dialog(&mut self, ctx: &egui::Context) {
        let all: Vec<&str> = [SUPPORTED_EXTENSIONS, sources::ARCHIVE_EXTENSIONS].concat();
        let picked = rfd::FileDialog::new()
            .add_filter("3D files and archives", &all)
            .add_filter("3D files", SUPPORTED_EXTENSIONS)
            .add_filter("Archives", sources::ARCHIVE_EXTENSIONS)
            .pick_files();
        self.open_files(ctx, picked.unwrap_or_default());
    }

    fn open_folder_dialog(&mut self, ctx: &egui::Context) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            let _ = self.set_browse_root(Some(folder.clone()));
            self.panel_tab = PanelTab::Files;
            self.show_objects = true;
            self.open_files(ctx, [folder]);
        }
    }

    /// List a folder, or the models of an archive, in the Files tab.
    fn set_browse_root(&mut self, root: Option<PathBuf>) -> Result<(), String> {
        self.selected.clear();
        self.archive_models = None;
        let mut result = Ok(());
        self.tree = match root.as_deref() {
            Some(r) if r.is_file() && sources::is_archive(r) => match sources::list_models(r) {
                Ok(entries) => {
                    let tree = sources::archive_tree(r, &entries);
                    self.archive_models = Some((r.to_path_buf(), entries));
                    Some(tree)
                }
                Err(e) => {
                    self.status = format!("Could not open {}: {e}", r.display());
                    result = Err(e);
                    None
                }
            },
            Some(r) => Some(sources::scan(r, TREE_DEPTH)),
            None => None,
        };
        self.browse_root = root;
        self.panel_tab = if self.archive_models.is_some() { PanelTab::Files } else { self.panel_tab };
        result
    }

    /// Virtual path of the first model of the archive listed in the Files tab.
    fn first_model(&self, archive: &Path) -> Option<PathBuf> {
        match &self.archive_models {
            Some((listed, entries)) if listed == archive => entries.first().map(|e| archive.join(e)),
            _ => None,
        }
    }

    // --- appearance --------------------------------------------------------------------------------

    /// Number of models (not supports) in the scene so far.
    fn model_count(&self) -> usize {
        self.entries.iter().filter(|e| e.kind == ObjectKind::Model).count()
    }

    /// Style for an object; `ordinal` is its position among the models (picks its colour when colouring each).
    fn style(&self, kind: ObjectKind, ordinal: usize, settings: &Settings) -> Style {
        let (mut color, name) = match kind {
            ObjectKind::Support => (settings.support_color, &settings.support_material),
            ObjectKind::Model => (settings.model_color, &settings.model_material),
        };
        if self.color_models && kind == ObjectKind::Model {
            color = MODEL_COLORS[ordinal % MODEL_COLORS.len()];
        }
        Style { color: rgb(color), material: material(name).unwrap_or(MATERIALS[1].1) }
    }

    fn apply_styles(&mut self, settings: &Settings) {
        let mut models = 0;
        for entry in &self.entries {
            let style = self.style(entry.kind, models, settings);
            models += (entry.kind == ObjectKind::Model) as usize;
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
        if ctx.input_mut(|i| i.consume_shortcut(&CONTROLS)) {
            self.show_controls = !self.show_controls;
        }
        if ctx.wants_keyboard_input() {
            return; // typing in a text field
        }
        let pressed = |key| ctx.input(|i| i.key_pressed(key) && i.modifiers.is_none());
        if pressed(Key::Escape) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if pressed(Key::F) {
            self.viewport.fit_all();
        }
        if pressed(Key::ArrowLeft) {
            self.step_file(ctx, -1);
        }
        if pressed(Key::ArrowRight) {
            self.step_file(ctx, 1);
        }
        if pressed(Key::H) {
            self.toggle_all_visible();
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
        let supported: Vec<PathBuf> =
            dropped.into_iter().filter(|p| p.is_dir() || sources::is_model(p) || sources::is_archive(p)).collect();
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
                if ui.button("Open folder…").clicked() {
                    self.open_folder_dialog(&ctx);
                }
                ui.menu_button("Open Recent", |ui| self.recent_menu(ui, &ctx));
                if ui.add(egui::Button::new("Clear scene").shortcut_text(ctx.format_shortcut(&CLEAR))).clicked() {
                    self.clear();
                }
                if ui.add(egui::Button::new("Preferences…").shortcut_text(ctx.format_shortcut(&PREFERENCES))).clicked()
                {
                    self.draft = Some(self.settings.clone());
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
                ui.checkbox(&mut self.show_toolbar, "Button bar");
                if ui.add(egui::Button::new("Hide / show all objects").shortcut_text("H")).clicked() {
                    self.toggle_all_visible();
                }
                let mut colored = self.color_models;
                if ui.checkbox(&mut colored, "Colour each model").changed() {
                    self.set_color_models(colored);
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.add(egui::Button::new("Controls").shortcut_text(ctx.format_shortcut(&CONTROLS))).clicked() {
                    self.show_controls = true;
                }
                if ui.button("User guide").clicked() {
                    self.show_guide = true;
                }
                ui.separator();
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

    /// Hide everything, or show everything again when nothing is visible.
    fn toggle_all_visible(&mut self) {
        let show = !self.entries.iter().any(|e| e.visible);
        for entry in &mut self.entries {
            entry.visible = show;
            self.viewport.renderer.set_visible(entry.id, show);
        }
    }

    fn set_color_models(&mut self, on: bool) {
        self.color_models = on;
        self.apply_styles(&self.settings.clone());
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let text = self.draft.as_ref().unwrap_or(&self.settings).toolbar_text; // Preferences previews live
        let ctx = ui.ctx().clone();
        let icons = &self.tool_icons;
        let (open, folder, clear, objects, eye, colours) =
            (&icons.open, &icons.folder, &icons.clear, &icons.objects, &icons.eye, &icons.colours);
        let (show_objects, color_models) = (self.show_objects, self.color_models);
        let button = |ui: &mut egui::Ui, icon: &egui::TextureHandle, label: &str, on: bool, hint: &str| {
            tool_button(ui, icon, label, text, on).on_hover_text(hint).clicked()
        };
        let mut clicked = [false; 6];
        ui.horizontal(|ui| {
            clicked[0] = button(ui, open, "Open", false, "Open files (Ctrl+O)");
            clicked[1] = button(ui, folder, "Folder", false, "Open all models in a folder");
            clicked[2] = button(ui, clear, "Clear", false, "Clear scene: remove everything (Ctrl+W)");
            ui.separator();
            clicked[3] = button(ui, objects, "Objects", show_objects, "Show/hide the objects panel");
            clicked[4] = button(ui, eye, "Hide all", false, "Hide or show all objects (H)");
            clicked[5] = button(ui, colours, "Colours", color_models, "Give every model its own colour");
        });
        if clicked[0] {
            self.open_dialog(&ctx);
        }
        if clicked[1] {
            self.open_folder_dialog(&ctx);
        }
        if clicked[2] {
            self.clear();
        }
        if clicked[3] {
            self.show_objects = !self.show_objects;
        }
        if clicked[4] {
            self.toggle_all_visible();
        }
        if clicked[5] {
            self.set_color_models(!self.color_models);
        }
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

    fn files_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            if ui.button("Folder…").clicked() {
                self.open_folder_dialog(&ctx);
            }
            let can_open = !self.selected.is_empty();
            if ui.add_enabled(can_open, egui::Button::new(format!("Open selected ({})", self.selected.len()))).clicked()
            {
                let picked: Vec<PathBuf> = std::mem::take(&mut self.selected).into_iter().collect();
                self.open_files(&ctx, picked);
            }
            if ui.add_enabled(self.browse_root.is_some(), egui::Button::new("⟳")).on_hover_text("Refresh").clicked() {
                let root = self.browse_root.clone();
                let _ = self.set_browse_root(root);
            }
        });
        let Some(tree) = self.tree.take() else {
            ui.weak("Open a folder or a file to list the models next to it.");
            return;
        };
        let mut to_open = None;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            if let Some(root) = &self.browse_root {
                ui.label(RichText::new(root.display().to_string()).weak());
            }
            if tree.children.is_empty() {
                ui.weak("No model files here.");
            }
            for child in &tree.children {
                self.tree_node(ui, child, &mut to_open);
            }
        });
        self.tree = Some(tree);
        if let Some(path) = to_open {
            self.selected.remove(&path);
            self.open_files(&ctx, [path]);
        }
    }

    fn tree_node(&mut self, ui: &mut egui::Ui, node: &Node, to_open: &mut Option<PathBuf>) {
        let name = file_name(&node.path);
        if node.is_dir {
            egui::CollapsingHeader::new(format!("📁 {name}"))
                .id_salt((self.archive_models.is_some(), &node.path)) // own state: archive folders start open
                .default_open(self.archive_models.is_some())
                .show(ui, |ui| {
                    if node.path.is_dir() {
                        // a real folder (not a folder inside an archive)
                        ui.horizontal(|ui| {
                            if ui.small_button("Open folder").clicked() {
                                *to_open = Some(node.path.clone());
                            }
                        });
                    }
                    for child in &node.children {
                        self.tree_node(ui, child, to_open);
                    }
                });
        } else {
            let mut ticked = self.selected.contains(&node.path);
            let label = if sources::is_archive(&node.path) { format!("📦 {name}") } else { name };
            let response = ui.checkbox(&mut ticked, label).on_hover_text(node.path.display().to_string());
            if response.changed() {
                if ticked {
                    self.selected.insert(node.path.clone());
                } else {
                    self.selected.remove(&node.path);
                }
            }
            if response.double_clicked() {
                *to_open = Some(node.path.clone());
            }
        }
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
            ui.label(&entry.name).on_hover_text(tooltip);
            // Models get a centre-on icon; only one model can be centred at a time.
            if entry.kind == ObjectKind::Model && entry.parent.is_none() {
                let id = entry.id;
                let centred = self.viewport.focus == Some(id);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let info =
                        ui.add(egui::Button::new("ℹ").small().frame(false)).on_hover_text("Show model information");
                    if info.clicked() {
                        self.info_for = if self.info_for == Some(index) { None } else { Some(index) };
                    }
                    let hint = if centred {
                        "Centred on this model: rotation pivots around it. Click to show the whole scene again."
                    } else {
                        "Centre the view on this model and rotate around it"
                    };
                    if focus_icon(ui, centred).on_hover_text(hint).clicked() {
                        if centred {
                            self.viewport.fit_all();
                        } else {
                            self.viewport.center_on(id);
                        }
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
                ui.label("Button bar:");
                ui.checkbox(&mut draft.toolbar_text, "Show text under the icons");
                ui.end_row();
                ui.label("Theme:");
                combo(ui, "theme", &mut draft.theme, &THEMES);
                ui.end_row();
                ui.label("Mouse orbit:");
                combo(ui, "mouse orbit", &mut draft.mouse_orbit, &ORBIT_STYLES);
                ui.end_row();
                ui.label("LYS files:");
                ui.checkbox(&mut draft.show_plate, "Show build plate");
                ui.end_row();
                if cfg!(windows) {
                    ui.label("File types:");
                    let hint = "Windows decides which program opens STL, OBJ, PLY and LYS files: pick MeshPeek there";
                    if ui.button("Make default app…").on_hover_text(hint).clicked() {
                        open_default_apps_settings();
                    }
                    ui.end_row();
                }
                ui.label("Space mouse:");
                ui.checkbox(&mut draft.space_mouse.enabled, "Enabled");
                ui.end_row();
                ui.label("    Sensitivity:");
                ui.add(egui::Slider::new(&mut draft.space_mouse.sensitivity, 0.1..=5.0).logarithmic(true));
                ui.end_row();
                ui.label("    Invert:");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut draft.space_mouse.invert_pan, "Pan");
                    ui.checkbox(&mut draft.space_mouse.invert_zoom, "Zoom");
                    ui.checkbox(&mut draft.space_mouse.invert_rotation, "Rotation");
                });
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

    fn info_window(&mut self, ctx: &egui::Context) {
        let Some(index) = self.info_for else { return };
        let Some(entry) = self.entries.get(index) else {
            self.info_for = None;
            return;
        };
        let supports: usize = self.entries.iter().filter(|e| e.parent == Some(index)).map(|e| e.triangles).sum();
        let size = self.viewport.renderer.object_bounds(entry.id).map(|(lo, hi)| hi - lo);
        let mut open = true;
        egui::Window::new(format!("Information: {}", entry.name)).collapsible(false).open(&mut open).show(ctx, |ui| {
            egui::Grid::new("model info").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Path:");
                ui.label(entry.source.display().to_string());
                ui.end_row();
                ui.label("Triangles:");
                ui.label(thousands(entry.triangles));
                ui.end_row();
                if supports > 0 {
                    ui.label("Support triangles:");
                    ui.label(thousands(supports));
                    ui.end_row();
                }
                if let Some(s) = size {
                    ui.label("Width (X):");
                    ui.label(format!("{:.2} mm", s.x));
                    ui.end_row();
                    ui.label("Depth (Y):");
                    ui.label(format!("{:.2} mm", s.y));
                    ui.end_row();
                    ui.label("Height (Z):");
                    ui.label(format!("{:.2} mm", s.z));
                    ui.end_row();
                }
                for (label, value) in &entry.info {
                    ui.label(format!("{label}:"));
                    ui.label(value);
                    ui.end_row();
                }
            });
        });
        if !open {
            self.info_for = None;
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("About MeshPeek").collapsible(false).resizable(false).open(&mut self.show_about).show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.tool_icons.app).fit_to_exact_size(egui::vec2(128.0, 128.0)));
                    ui.vertical(|ui| {
                        ui.heading(format!("MeshPeek {}", env!("CARGO_PKG_VERSION")));
                        ui.label("Fast viewer for STL, OBJ, PLY and Lychee .lys files.");
                        ui.label("by MrIxolite");
                    });
                });
                ui.separator();
                ui.label(format!("Rendering with: {}", self.adapter));
                ui.label(format!("Memory in use: {:.0} MB", memory_mb()));
                ui.label(format!("Space mouse: {}", self.space_device.as_deref().unwrap_or("none detected")));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("License:");
                    ui.hyperlink_to(LICENSE_NAME, LICENSE_URL);
                });
                match third_party_file() {
                    Some(path) => {
                        if ui.link("Third-party licenses").clicked() {
                            let url = format!("file:///{}", path.display().to_string().replace('\\', "/"));
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        }
                    }
                    None => {
                        ui.weak(format!("Third-party licenses: {THIRD_PARTY_FILE} (included with the installed app)"));
                    }
                }
            },
        );
    }
}

impl eframe::App for MeshPeekApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive_loads();
        self.shortcuts(ctx);
        self.apply_theme(ctx);
        self.navigate(ctx);

        egui::TopBottomPanel::top("menu").show(ctx, |ui| self.menu_bar(ui));
        if self.show_toolbar {
            egui::TopBottomPanel::top("toolbar").show(ctx, |ui| self.toolbar(ui));
        }
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
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.panel_tab, PanelTab::Objects, "Objects");
                    ui.selectable_value(&mut self.panel_tab, PanelTab::Files, "Files");
                });
                ui.separator();
                match self.panel_tab {
                    PanelTab::Objects => self.objects_panel(ui),
                    PanelTab::Files => self.files_panel(ui),
                }
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let render_state = self.render_state.clone();
            let settings = self.draft.as_ref().unwrap_or(&self.settings); // Preferences previews live
            self.viewport.turntable = settings.mouse_orbit == "Turntable";
            self.viewport.ui(ui, &render_state);
        });
        self.preferences_window(ctx);
        self.info_window(ctx);
        self.about_window(ctx);
        help::controls_window(ctx, &mut self.show_controls);
        help::guide_window(ctx, &mut self.show_guide);
        self.snapshot(ctx);
    }
}

impl MeshPeekApp {
    /// Follow the theme setting in the UI and in the 3D view.
    fn apply_theme(&mut self, ctx: &egui::Context) {
        let settings = self.draft.as_ref().unwrap_or(&self.settings); // Preferences previews live
        self.viewport.renderer.set_show_plates(settings.show_plate);
        let preference = match settings.theme.as_str() {
            "Light" => egui::ThemePreference::Light,
            "Dark" => egui::ThemePreference::Dark,
            _ => egui::ThemePreference::System,
        };
        if ctx.options(|o| o.theme_preference) != preference {
            ctx.set_theme(preference);
        }
        let palette = if ctx.theme() == egui::Theme::Light { Palette::LIGHT } else { Palette::DARK };
        self.viewport.renderer.set_palette(palette);
    }

    /// Keyboard and space mouse navigation; keeps repainting while either is moving the view.
    fn navigate(&mut self, ctx: &egui::Context) {
        let mut moving = self.viewport.keyboard(ctx);
        let (motion, pressed, device) = {
            let mut state = self.space_mouse.lock().unwrap();
            let fresh = state.updated.is_some_and(|t| t.elapsed() < spacemouse::STALE);
            let motion = if fresh { state.motion } else { Motion::default() };
            (motion, std::mem::take(&mut state.pressed), state.device.clone())
        };
        self.space_device = device;
        // While Preferences is open, preview its space mouse settings.
        let settings = self.draft.as_ref().unwrap_or(&self.settings).space_mouse;
        if settings.enabled {
            if pressed & 1 != 0 {
                self.viewport.fit_all();
            }
            if pressed & 2 != 0 {
                self.viewport.show_view(View::Isometric);
            }
            let dt = ctx.input(|i| i.stable_dt.min(0.1)) as f64;
            moving |= self.viewport.space_mouse(motion, &settings, dt);
        }
        if moving {
            ctx.request_repaint();
        }
    }

    /// Test hook: `MESHPEEK_SNAPSHOT_ACTIONS="theme=Light;center=Bust;view=Front;orbit=90,0;pan=100,0;dolly=4;controls;guide;about;preferences"` is
    /// applied once loading finished, before the snapshot.
    fn snapshot_actions(&mut self) {
        let Ok(actions) = std::env::var("MESHPEEK_SNAPSHOT_ACTIONS") else { return };
        for action in actions.split(';').map(str::trim) {
            match action.split_once('=') {
                Some(("theme", theme)) => self.settings.theme = theme.to_string(),
                Some(("center", name)) => {
                    if let Some(e) = self.entries.iter().find(|e| e.name == name && e.parent.is_none()) {
                        self.viewport.center_on(e.id);
                    }
                }
                Some(("view", name)) => {
                    if let Some((view, ..)) = View::ALL.iter().find(|(_, label, _)| label.eq_ignore_ascii_case(name)) {
                        self.viewport.show_view(*view);
                    }
                }
                Some(("orbit", degrees)) => {
                    let (azimuth, elevation) = degrees.split_once(',').unwrap_or((degrees, "0"));
                    self.viewport
                        .camera
                        .orbit_turntable(azimuth.parse().unwrap_or(0.0), elevation.parse().unwrap_or(0.0));
                }
                Some(("pan", pixels)) => {
                    let (dx, dy) = pixels.split_once(',').unwrap_or((pixels, "0"));
                    self.viewport.pan_pixels(dx.parse().unwrap_or(0.0), dy.parse().unwrap_or(0.0));
                }
                Some(("dolly", factor)) => {
                    self.viewport.camera.dolly(factor.parse().unwrap_or(1.0));
                }
                _ => match action {
                    "controls" => self.show_controls = true,
                    "guide" => self.show_guide = true,
                    "about" => self.show_about = true,
                    "preferences" => self.draft = Some(self.settings.clone()),
                    _ => {}
                },
            }
        }
    }

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
            if *frames == 1 {
                self.snapshot_actions();
            } else if *frames == 8 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
        ctx.request_repaint();
    }
}

/// Crosshair icon for centring on a model: an outline when not centred, filled with the accent colour when
/// centred.
fn focus_icon(ui: &mut egui::Ui, centred: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
    let visuals = ui.visuals();
    let color = if centred {
        visuals.selection.bg_fill
    } else if response.hovered() {
        visuals.strong_text_color()
    } else {
        visuals.weak_text_color()
    };
    let painter = ui.painter();
    let (c, r) = (rect.center(), 5.0);
    let stroke = Stroke::new(1.5_f32, color);
    if centred {
        painter.circle_filled(c, r, color);
        painter.circle_filled(c, 1.8, Color32::WHITE);
    } else {
        painter.circle_stroke(c, r, stroke);
    }
    for d in [egui::vec2(1.0, 0.0), egui::vec2(-1.0, 0.0), egui::vec2(0.0, 1.0), egui::vec2(0.0, -1.0)] {
        painter.line_segment([c + d * (r + 0.5), c + d * (r + 3.5)], stroke);
    }
    response
}

/// Toolbar button: the coloured icon centred above its label (or the icon alone); `on` draws it pressed in.
fn tool_button(
    ui: &mut egui::Ui,
    icon: &egui::TextureHandle,
    label: &str,
    show_text: bool,
    on: bool,
) -> egui::Response {
    const ICON: f32 = 28.0;
    const PAD: f32 = 5.0;
    let font = egui::FontId::proportional(11.0);
    let color = ui.visuals().text_color();
    let galley = show_text.then(|| ui.painter().layout_no_wrap(label.to_string(), font, color));
    let text_size = galley.as_ref().map_or(egui::Vec2::ZERO, |g| g.size());
    let width = (ICON.max(text_size.x) + 2.0 * PAD).max(if show_text { 52.0 } else { 0.0 });
    let height = ICON + 2.0 * PAD + if show_text { text_size.y + 2.0 } else { 0.0 };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let fill = if on {
            visuals.selection.bg_fill
        } else if response.is_pointer_button_down_on() {
            visuals.widgets.active.bg_fill
        } else if response.hovered() {
            visuals.widgets.hovered.bg_fill
        } else {
            Color32::TRANSPARENT
        };
        let painter = ui.painter();
        painter.rect_filled(rect, 4.0, fill);
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.top() + PAD + ICON / 2.0),
            egui::vec2(ICON, ICON),
        );
        painter.image(
            icon.id(),
            icon_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if let Some(galley) = galley {
            let at = egui::pos2(rect.center().x - galley.size().x / 2.0, icon_rect.bottom() + 2.0);
            painter.galley(at, galley, color);
        }
    }
    response
}

/// Windows does not let programs set file associations themselves; open its "Default apps" page instead.
fn open_default_apps_settings() {
    if let Err(e) = std::process::Command::new("explorer.exe").arg("ms-settings:defaultapps").spawn() {
        log::warn!("could not open the Default apps settings: {e}");
    }
}

/// The third-party notices file next to the executable (zip, MSI) or in /usr/share/doc (Linux package).
fn third_party_file() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [exe_dir.join(THIRD_PARTY_FILE), PathBuf::from("/usr/share/doc/meshpeek").join(THIRD_PARTY_FILE)]
        .into_iter()
        .find(|p| p.exists())
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
