//! The app's own windows: Preferences, model information and About.

use std::path::PathBuf;

use eframe::egui;

use super::{MeshPeekApp, memory_mb, thousands};
use crate::preferences;

/// License of MeshPeek itself.
const LICENSE_NAME: &str = "CC BY-NC-SA 4.0";
/// Where the licence text is published.
const LICENSE_URL: &str = "https://creativecommons.org/licenses/by-nc-sa/4.0/";
/// Notices of the open-source libraries built into MeshPeek; shipped next to the executable.
const THIRD_PARTY_FILE: &str = "THIRD-PARTY-LICENSES.html";

impl MeshPeekApp {
    /// The Preferences window: edits a draft that is previewed live and kept on OK.
    pub(super) fn preferences_window(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.draft.take() else { return };
        let mut decision = None; // Some(true) = OK, Some(false) = Cancel
        let mut open = true;
        let section = &mut self.preferences_section;
        egui::Window::new("Preferences")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                preferences::show(ui, &mut draft, section);
                ui.separator();
                let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
                ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                    if ui.button("OK").clicked() {
                        decision = Some(true);
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

    /// Facts about one object and its file.
    pub(super) fn info_window(&mut self, ctx: &egui::Context) {
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

    /// Version, author, licences, GPU and space mouse.
    pub(super) fn about_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("About MeshPeek").collapsible(false).resizable(false).open(&mut self.show_about).show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.tool_icons.app).fit_to_exact_size(egui::vec2(128.0, 128.0)));
                    ui.vertical(|ui| {
                        ui.heading(format!("MeshPeek {}", env!("CARGO_PKG_VERSION")));
                        ui.label("Fast viewer for STL, OBJ, PLY, 3MF, Lychee .lys, CHITUBOX .chitubox, .ctp and .ctb, and RuneBrace .ags files.");
                        ui.label(
                            "Lychee and CHITUBOX scenes are shown with their supports, 3MF files with their colours, CTB print files as the solid their layers print.",
                        );
                        ui.label("by MrIxolite");
                        // the project page, from the workspace's `repository` field
                        let repository = env!("CARGO_PKG_REPOSITORY");
                        ui.hyperlink_to(repository.trim_start_matches("https://"), repository)
                            .on_hover_text("Source code, releases and issues on GitHub");
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

/// The third-party notices file next to the executable (zip, MSI) or in /usr/share/doc (Linux package).
fn third_party_file() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [exe_dir.join(THIRD_PARTY_FILE), PathBuf::from("/usr/share/doc/meshpeek").join(THIRD_PARTY_FILE)]
        .into_iter()
        .find(|p| p.exists())
}
