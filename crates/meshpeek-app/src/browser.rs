//! The Files tab: a file browser over the file system. Folders and archives expand in place (their contents are
//! read when first shown); double-clicking a model adds it to the scene, double-clicking a folder or an archive
//! makes it the current location. Only folders, archives and model files are listed.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use eframe::egui;

use crate::sources;

/// What a row of the browser is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A folder on disk, a folder inside an archive, or a drive.
    Folder,
    /// A ZIP, 7z or RAR file: browsed like a folder.
    Archive,
    /// A file MeshPeek can load (on disk or inside an archive).
    Model,
}

/// One row: a path and what it is.
#[derive(Debug, Clone)]
pub struct Item {
    pub path: PathBuf,
    pub kind: Kind,
}

/// What the user asked the browser for; the app carries it out.
pub enum Action {
    /// Add these model files to the scene.
    Add(Vec<PathBuf>),
    /// Ask for a folder to browse.
    PickFolder,
}

/// The browser's location, cached listings and selection.
#[derive(Default)]
pub struct Browser {
    /// The folder or archive (or folder inside an archive) shown; `None` lists the drives (Windows).
    root: Option<PathBuf>,
    /// Contents of folders, keyed by path: read once, until Refresh.
    listings: HashMap<PathBuf, Result<Vec<Item>, String>>,
    /// Model entries of archives (paths inside, with `/`), keyed by archive.
    archives: HashMap<PathBuf, Result<Vec<String>, String>>,
    /// Selected model files.
    selected: BTreeSet<PathBuf>,
    /// The file a Shift+click range starts from.
    anchor: Option<PathBuf>,
    /// Model files in the order they were drawn last frame (for Shift+click ranges).
    order: Vec<PathBuf>,
}

impl Browser {
    /// The current location.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Show `root` (a folder, an archive or a folder inside an archive) and clear the selection.
    pub fn set_root(&mut self, root: Option<PathBuf>) {
        if self.root != root {
            self.selected.clear();
            self.anchor = None;
        }
        self.root = root;
    }

    /// Forget all cached listings, so folders and archives are read again.
    pub fn refresh(&mut self) {
        self.listings.clear();
        self.archives.clear();
    }

    /// The model entries of `archive` (paths inside it, with `/`, sorted), read once.
    pub fn archive_entries(&mut self, archive: &Path) -> Result<Vec<String>, String> {
        self.archives.entry(archive.to_path_buf()).or_insert_with(|| sources::list_models(archive)).clone()
    }

    /// The folder above the current location (the archive's folder for an archive); on Windows the drive list
    /// above a drive.
    fn parent(&self) -> Option<Option<PathBuf>> {
        let root = self.root.as_ref()?;
        match root.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => Some(Some(parent.to_path_buf())),
            _ if cfg!(windows) => Some(None),
            _ => None,
        }
    }

    /// What `path` contains: sub-folders first, then archives and model files, each sorted by name.
    fn children(&mut self, path: &Path) -> Result<Vec<Item>, String> {
        if let Some(listing) = self.listings.get(path) {
            return listing.clone();
        }
        let listing = if path.is_dir() {
            read_folder(path)
        } else {
            // an archive, or a folder inside one
            let (archive, prefix) = match sources::archive_of(path) {
                Some((archive, prefix)) => (archive, prefix),
                None => (path.to_path_buf(), String::new()),
            };
            self.archive_entries(&archive).map(|entries| archive_folder(&archive, &prefix, &entries))
        };
        self.listings.insert(path.to_path_buf(), listing.clone());
        listing
    }

    /// The model files under `path`: directly in it, or (`recursive`) also in its sub-folders and archives.
    fn models_in(&mut self, path: &Path, recursive: bool) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for item in self.children(path).unwrap_or_default() {
            match item.kind {
                Kind::Model => found.push(item.path),
                Kind::Folder | Kind::Archive if recursive => found.extend(self.models_in(&item.path, true)),
                _ => {}
            }
        }
        found
    }

    /// Draw the tab; returns what the user asked for.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut actions = Vec::new();
        let inside_archive = self.root.as_deref().is_some_and(|r| !r.is_dir());
        ui.horizontal_wrapped(|ui| {
            let up = self.parent();
            if ui.add_enabled(up.is_some(), egui::Button::new("⬆ Up")).on_hover_text("Go up one folder").clicked() {
                self.set_root(up.flatten());
            }
            let selected = self.selected.len();
            let add_selected = egui::Button::new(format!("Add selected ({selected})"));
            if ui
                .add_enabled(selected > 0, add_selected)
                .on_hover_text("Add the selected files to the scene (Ctrl+click and Shift+click select several)")
                .clicked()
            {
                actions.push(Action::Add(std::mem::take(&mut self.selected).into_iter().collect()));
            }
            let all_hint = if inside_archive {
                "Add every model of this archive folder, sub-folders included"
            } else {
                "Add every model file in this folder (not in its sub-folders)"
            };
            if ui.add_enabled(self.root.is_some(), egui::Button::new("Add all")).on_hover_text(all_hint).clicked() {
                if let Some(root) = self.root.clone() {
                    let models = self.models_in(&root, inside_archive);
                    if !models.is_empty() {
                        actions.push(Action::Add(models));
                    }
                }
            }
            if ui.button("🗁 Browse…").on_hover_text("Choose a folder to browse").clicked() {
                actions.push(Action::PickFolder);
            }
            if ui.button("⟳").on_hover_text("Read the folders again").clicked() {
                self.refresh();
            }
        });
        let location = self.root.as_ref().map_or_else(|| "Drives".to_string(), |r| r.display().to_string());
        ui.label(egui::RichText::new(location).weak()).on_hover_text("Double-click a folder or archive to go into it");
        ui.separator();

        let items = match self.root.clone() {
            Some(root) => self.children(&root),
            None => Ok(drives()),
        };
        let mut order = Vec::new();
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| match items {
            Err(e) => {
                ui.colored_label(ui.visuals().warn_fg_color, format!("Cannot read this location: {e}"));
            }
            Ok(items) if items.is_empty() => {
                ui.weak("No folders or model files here.");
            }
            Ok(items) => {
                for item in &items {
                    self.row(ui, item, &mut order, &mut actions);
                }
            }
        });
        self.order = order;
        actions
    }

    /// One row; folders and archives expand to their contents (read when first opened).
    fn row(&mut self, ui: &mut egui::Ui, item: &Item, order: &mut Vec<PathBuf>, actions: &mut Vec<Action>) {
        let name = display_name(&item.path);
        match item.kind {
            Kind::Folder | Kind::Archive => {
                let icon = if item.kind == Kind::Folder { "🗀" } else { "📦" };
                let response = egui::CollapsingHeader::new(format!("{icon} {name}"))
                    .id_salt(&item.path)
                    .default_open(false)
                    .show(ui, |ui| match self.children(&item.path) {
                        Err(e) => {
                            ui.colored_label(ui.visuals().warn_fg_color, e);
                        }
                        Ok(children) if children.is_empty() => {
                            ui.weak("(no model files)");
                        }
                        Ok(children) => {
                            for child in &children {
                                self.row(ui, child, order, actions);
                            }
                        }
                    });
                let header = response.header_response.on_hover_text(item.path.display().to_string());
                if header.double_clicked() {
                    self.set_root(Some(item.path.clone()));
                }
            }
            Kind::Model => {
                order.push(item.path.clone());
                let selected = self.selected.contains(&item.path);
                let response = ui
                    .selectable_label(selected, format!("🗋 {name}"))
                    .on_hover_text(format!("{}\nDouble-click to add it to the scene", item.path.display()));
                if response.double_clicked() {
                    actions.push(Action::Add(vec![item.path.clone()]));
                } else if response.clicked() {
                    let modifiers = ui.input(|i| i.modifiers);
                    self.click(&item.path, modifiers);
                }
            }
        }
    }

    /// Selection by click: alone, toggled (Ctrl/Cmd) or as a range from the last clicked file (Shift).
    fn click(&mut self, path: &Path, modifiers: egui::Modifiers) {
        if modifiers.shift {
            if let Some(range) = self.anchor.as_ref().and_then(|anchor| range(&self.order, anchor, path)) {
                self.selected = range.into_iter().collect();
                return;
            }
        }
        if modifiers.command {
            if !self.selected.remove(path) {
                self.selected.insert(path.to_path_buf());
            }
        } else {
            self.selected = BTreeSet::from([path.to_path_buf()]);
        }
        self.anchor = Some(path.to_path_buf());
    }
}

/// The files of `order` from `a` to `b` (either way round), if both are listed.
fn range(order: &[PathBuf], a: &Path, b: &Path) -> Option<Vec<PathBuf>> {
    let (i, j) = (order.iter().position(|p| p == a)?, order.iter().position(|p| p == b)?);
    Some(order[i.min(j)..=i.max(j)].to_vec())
}

/// The last component of a path (a drive keeps its whole name).
fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Sub-folders, archives and model files of a folder on disk. Hidden and system entries (names starting with `.`
/// or `$`) are left out.
fn read_folder(dir: &Path) -> Result<Vec<Item>, String> {
    let mut items: Vec<Item> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            !(name.starts_with('.') || name.starts_with('$') || name == "System Volume Information")
        })
        .filter_map(|entry| {
            let path = entry.path();
            let kind = if path.is_dir() {
                Kind::Folder
            } else if sources::is_archive(&path) {
                Kind::Archive
            } else if sources::is_model(&path) {
                Kind::Model
            } else {
                return None;
            };
            Some(Item { path, kind })
        })
        .collect();
    sort(&mut items);
    Ok(items)
}

/// The rows of the folder `prefix` (empty = top level) inside `archive`, from its model `entries`.
fn archive_folder(archive: &Path, prefix: &str, entries: &[String]) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    for entry in entries {
        let rest = if prefix.is_empty() {
            entry.as_str()
        } else {
            match entry.strip_prefix(prefix).and_then(|r| r.strip_prefix('/')) {
                Some(rest) => rest,
                None => continue,
            }
        };
        let (first, deeper) = match rest.split_once('/') {
            Some((first, _)) => (first, true),
            None => (rest, false),
        };
        let path = archive.join(prefix).join(first);
        if !items.iter().any(|i| i.path == path) {
            items.push(Item { path, kind: if deeper { Kind::Folder } else { Kind::Model } });
        }
    }
    sort(&mut items);
    items
}

/// Folders first, then archives and models, each by name ignoring case.
fn sort(items: &mut [Item]) {
    items.sort_by_key(|i| (i.kind == Kind::Model || i.kind == Kind::Archive, display_name(&i.path).to_lowercase()));
}

/// The drives of the computer (Windows); empty elsewhere.
fn drives() -> Vec<Item> {
    if !cfg!(windows) {
        return Vec::new();
    }
    (b'A'..=b'Z')
        .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
        .filter(|drive| drive.exists())
        .map(|path| Item { path, kind: Kind::Folder })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_folders_list_one_level() {
        let archive = Path::new("pack.zip");
        let entries = ["a.stl", "inner/b.obj", "inner/deep/c.ply"].map(String::from);
        let top = archive_folder(archive, "", &entries);
        assert_eq!(
            top.iter().map(|i| (display_name(&i.path), i.kind)).collect::<Vec<_>>(),
            [("inner".to_string(), Kind::Folder), ("a.stl".to_string(), Kind::Model)]
        );
        let inner = archive_folder(archive, "inner", &entries);
        assert_eq!(inner.len(), 2);
        assert_eq!(inner[0].kind, Kind::Folder);
        assert_eq!(inner[1].path, archive.join("inner").join("b.obj"));
    }

    #[test]
    fn folders_list_models_archives_and_sub_folders_only() {
        let dir = std::env::temp_dir().join("meshpeek-browser-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        for file in ["b.stl", "A.obj", "notes.txt", "pack.zip"] {
            std::fs::write(dir.join(file), "x").unwrap();
        }
        let names: Vec<String> = read_folder(&dir).unwrap().iter().map(|i| display_name(&i.path)).collect();
        assert_eq!(names, ["sub", "A.obj", "b.stl", "pack.zip"]);
    }

    #[test]
    fn shift_click_selects_a_range_in_display_order() {
        let order: Vec<PathBuf> = ["a", "b", "c", "d"].iter().map(PathBuf::from).collect();
        assert_eq!(range(&order, Path::new("c"), Path::new("a")).unwrap().len(), 3);
        assert!(range(&order, Path::new("x"), Path::new("a")).is_none());
    }
}
