//! The contents of the Preferences window: a list of sections on the left and the selected section's settings
//! on the right. The window itself (OK / Cancel, live preview) is in `app.rs`.

use eframe::egui;

use crate::settings::{MATERIALS, ORBIT_STYLES, SUPPORT_DETAIL, Settings, THEMES};

/// A page of the Preferences window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Section {
    /// Theme and button bar; material and colour of models and supports, support detail, OBJ axis, build plate.
    #[default]
    Appearance,
    /// Mouse orbit and the arrow keys.
    Navigation,
    /// 3Dconnexion devices.
    SpaceMouse,
    /// Integration with the operating system.
    System,
}

impl Section {
    /// The sections in display order (System only on Windows, where it has something to offer).
    fn all() -> Vec<Section> {
        let mut all = vec![Self::Appearance, Self::Navigation, Self::SpaceMouse];
        if cfg!(windows) {
            all.push(Self::System);
        }
        all
    }

    /// The section with this title, ignoring case (for the snapshot test hook).
    pub fn named(name: &str) -> Option<Section> {
        Self::all().into_iter().find(|s| s.title().eq_ignore_ascii_case(name))
    }

    /// Name in the section list and heading of the page.
    fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Navigation => "Navigation",
            Self::SpaceMouse => "Space mouse",
            Self::System => "System",
        }
    }
}

/// Width of the section list and size of the settings page, in points (fixed, so the window does not change
/// size when switching sections).
const LIST_WIDTH: f32 = 130.0;
const PAGE_SIZE: egui::Vec2 = egui::vec2(380.0, 345.0);

/// Draw the section list and the page of `section`, editing `draft`.
pub fn show(ui: &mut egui::Ui, draft: &mut Settings, section: &mut Section) {
    // A fixed-size area: the vertical separator would otherwise stretch the window to the screen's height.
    let size = egui::vec2(LIST_WIDTH + PAGE_SIZE.x + 16.0, PAGE_SIZE.y);
    ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Min), |ui| {
        ui.set_min_size(size);
        ui.vertical(|ui| {
            ui.set_width(LIST_WIDTH);
            for s in Section::all() {
                let button = egui::Button::selectable(*section == s, s.title()).min_size(egui::vec2(LIST_WIDTH, 24.0));
                if ui.add(button).clicked() {
                    *section = s;
                }
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_width(PAGE_SIZE.x);
            ui.heading(section.title());
            ui.add_space(6.0);
            page(ui, draft, *section);
        });
    });
}

/// The settings of one section, as a label / control grid.
fn page(ui: &mut egui::Ui, draft: &mut Settings, section: Section) {
    egui::Grid::new(("preferences", section)).num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| match section {
        Section::Appearance => {
            group(ui, "Interface");
            ui.label("Theme:");
            combo(ui, "theme", &mut draft.theme, &THEMES);
            ui.end_row();
            ui.label("Button bar:");
            ui.checkbox(&mut draft.toolbar_text, "Show text under the icons");
            ui.end_row();
            group(ui, "Models");
            ui.label("Material:");
            combo(ui, "model material", &mut draft.model_material, &material_names());
            ui.end_row();
            ui.label("Colour:");
            ui.color_edit_button_srgb(&mut draft.model_color).on_hover_text("Used when the file has no colours");
            ui.end_row();
            ui.label("OBJ files:");
            ui.checkbox(&mut draft.obj_y_up, "Y is up (Blender, Maya, ZBrush)")
                .on_hover_text("Rotate OBJ files so their Y axis points up. Applied by reloading open files.");
            ui.end_row();
            group(ui, "Supports");
            ui.label("Material:");
            combo(ui, "support material", &mut draft.support_material, &material_names());
            ui.end_row();
            ui.label("Colour:");
            ui.color_edit_button_srgb(&mut draft.support_color);
            ui.end_row();
            ui.label("Detail:");
            let details: Vec<&str> = SUPPORT_DETAIL.iter().map(|(n, _)| *n).collect();
            combo(ui, "support detail", &mut draft.support_detail, &details)
                .on_hover_text("Facets round each support. Lower is faster on large scenes; open files are reloaded.");
            ui.end_row();
            ui.label("Lychee files:");
            ui.checkbox(&mut draft.show_plate, "Show build plate");
            ui.end_row();
            ui.label("");
            ui.add_enabled(draft.show_plate, egui::Checkbox::new(&mut draft.show_volume, "Show printer volume"))
                .on_hover_text("The box above the plate showing the printer's build height");
            ui.end_row();
        }
        Section::Navigation => {
            ui.label("Mouse orbit:");
            combo(ui, "mouse orbit", &mut draft.mouse_orbit, &ORBIT_STYLES)
                .on_hover_text("Turntable turns around the vertical axis; trackball around the view's axes.");
            ui.end_row();
            ui.label("Arrow keys:");
            ui.checkbox(&mut draft.preload_neighbours, "Preload the previous and next file")
                .on_hover_text("Uses memory for two more models, but stepping through a folder is instant");
            ui.end_row();
        }
        Section::SpaceMouse => {
            ui.label("Space mouse:");
            ui.checkbox(&mut draft.space_mouse.enabled, "Enabled");
            ui.end_row();
            ui.add_enabled_ui(draft.space_mouse.enabled, |ui| ui.label("Sensitivity:"));
            ui.add_enabled(
                draft.space_mouse.enabled,
                egui::Slider::new(&mut draft.space_mouse.sensitivity, 0.1..=5.0).logarithmic(true),
            );
            ui.end_row();
            ui.add_enabled_ui(draft.space_mouse.enabled, |ui| ui.label("Invert:"));
            ui.add_enabled_ui(draft.space_mouse.enabled, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut draft.space_mouse.invert_pan, "Pan");
                    ui.checkbox(&mut draft.space_mouse.invert_zoom, "Zoom");
                    ui.checkbox(&mut draft.space_mouse.invert_rotation, "Rotation");
                });
            });
            ui.end_row();
        }
        Section::System => {
            ui.label("File types:");
            let hint =
                "Windows decides which program opens STL, OBJ, PLY, 3MF, LYS, CHITUBOX, CTP and CTB files: pick MeshPeek there";
            if ui.button("Make default app…").on_hover_text(hint).clicked() {
                crate::app::open_default_apps_settings();
            }
            ui.end_row();
        }
    });
}

/// A sub-heading row of a settings grid.
fn group(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).strong());
    ui.end_row();
}

/// A drop-down that sets `value` to one of `options`.
fn combo(ui: &mut egui::Ui, id: &str, value: &mut String, options: &[&str]) -> egui::Response {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.as_str())
        .show_ui(ui, |ui| {
            for option in options {
                ui.selectable_value(value, option.to_string(), *option);
            }
        })
        .response
}

/// Names of the [`MATERIALS`].
fn material_names() -> Vec<&'static str> {
    MATERIALS.iter().map(|(n, _)| *n).collect()
}
