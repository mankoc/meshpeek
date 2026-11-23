//! The Files tab: the current folder, with links up to the folder above (the drives at the top) and down into its
//! folders and archives (read when entered). A model's tick box puts it in the scene or takes it out. Only folders,
//! archives and model files are listed.

use std::collections::HashMap;
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
    /// Size of a model file on disk.
    pub size: Option<u64>,
}

/// What the user asked the browser for; the app carries it out.
pub enum Action {
    /// Add these model files to the scene.
    Add(Vec<PathBuf>),
    /// Take these model files out of the scene.
    Remove(Vec<PathBuf>),
    /// Ask for a folder to browse.
    PickFolder,
}

/// The browser's current folder and cached listings.
#[derive(Default)]
pub struct Browser {
    /// The current folder or archive (or folder inside an archive); `None` lists the drives.
    root: Option<PathBuf>,
    /// Contents of folders, keyed by path: read once, until Refresh.
    listings: HashMap<PathBuf, Result<Vec<Item>, String>>,
    /// Model entries of archives (paths inside, with `/`) and their sizes, keyed by archive.
    archives: HashMap<PathBuf, Result<Vec<(String, u64)>, String>>,
    /// The drives, found once (until Refresh): probing every letter can wait on slow or disconnected drives.
    drives: Option<Vec<Item>>,
}

impl Browser {
    /// The current location.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Go to `root` (a folder, an archive or a folder inside an archive; `None`: the drives).
    pub fn set_root(&mut self, root: Option<PathBuf>) {
        self.root = root.map(|r| std::path::absolute(&r).unwrap_or(r)); // relative paths have no folders above them
    }

    /// Forget all cached listings, so folders and archives are read again.
    pub fn refresh(&mut self) {
        self.listings.clear();
        self.archives.clear();
        self.drives = None;
    }

    /// The model entries of `archive` (paths inside it, with `/`, sorted), read once.
    pub fn archive_entries(&mut self, archive: &Path) -> Result<Vec<String>, String> {
        Ok(self.listed(archive)?.iter().map(|(name, _)| name.clone()).collect())
    }

    /// The model entries of `archive` with their sizes, read once.
    fn listed(&mut self, archive: &Path) -> Result<&Vec<(String, u64)>, String> {
        let listed = self.archives.entry(archive.to_path_buf()).or_insert_with(|| sources::list_models(archive));
        listed.as_ref().map_err(Clone::clone)
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
            self.listed(&archive).map(|entries| archive_folder(&archive, &prefix, entries))
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

    /// Draw the tab, ticking the files of `loaded` (files and folders in the scene); returns what the user asked for.
    pub fn ui(&mut self, ui: &mut egui::Ui, loaded: &[PathBuf]) -> Vec<Action> {
        let mut actions = Vec::new();
        let inside_archive = self.root.as_deref().is_some_and(|r| !r.is_dir());
        ui.horizontal_wrapped(|ui| {
            let all_hint = if inside_archive {
                "Add every model of this archive folder, sub-folders included"
            } else {
                "Add every model file in the current folder (not in its sub-folders)"
            };
            if ui.add_enabled(self.root.is_some(), egui::Button::new("Add all")).on_hover_text(all_hint).clicked()
                && let Some(root) = self.root.clone()
            {
                let models = self.models_in(&root, inside_archive);
                if !models.is_empty() {
                    actions.push(Action::Add(models));
                }
            }
            if ui.button("🗁 Browse…").on_hover_text("Choose a folder to go to").clicked() {
                actions.push(Action::PickFolder);
            }
            if ui.button("⟳").on_hover_text("Read the folders again").clicked() {
                self.refresh();
            }
        });
        let location = self.root.as_ref().map_or_else(|| "Drives".to_string(), |r| r.display().to_string());
        ui.label(egui::RichText::new(location).weak());
        ui.separator();

        let items = match self.root.clone() {
            Some(root) => self.children(&root),
            None => Ok(self.drives.get_or_insert_with(drives).clone()),
        };
        let inside = self.root.as_deref().is_some_and(|r| !r.is_dir());
        egui::ScrollArea::both().id_salt(&self.root).auto_shrink(false).show(ui, |ui| {
            if let Some(root) = self.root.clone() {
                let above = root.parent().map_or_else(|| "Drives".to_string(), |p| p.display().to_string());
                if ui.link("⬆ ..").on_hover_text(format!("Up to {above}")).clicked() {
                    self.set_root(root.parent().map(Path::to_path_buf));
                }
            }
            match items {
                Err(e) => {
                    ui.colored_label(ui.visuals().warn_fg_color, format!("Cannot read this folder: {e}"));
                }
                Ok(items) if items.is_empty() => {
                    ui.weak("No folders or model files here.");
                }
                Ok(items) => {
                    for item in &items {
                        self.row(ui, item, inside, loaded, &mut actions);
                    }
                }
            }
        });
        actions
    }

    /// One row of the current folder (`inside` an archive or not): folders and archives are links down into them.
    fn row(&mut self, ui: &mut egui::Ui, item: &Item, inside: bool, loaded: &[PathBuf], actions: &mut Vec<Action>) {
        let name = display_name(&item.path);
        match item.kind {
            Kind::Folder | Kind::Archive => {
                let icon = if item.kind == Kind::Folder { "🗀" } else { "📦" };
                let inside = inside || item.kind == Kind::Archive;
                // The tick box: a folder's own model files (sub-folders can be huge), all the models of an archive.
                // Only what was read already is counted: a folder must not read every folder and archive it lists.
                let known = inside && item.kind == Kind::Folder || self.listings.contains_key(&item.path);
                let models = if known { self.models_in(&item.path, inside) } else { Vec::new() };
                let is_shown = |p: &Path| loaded.iter().any(|l| p.starts_with(l));
                let shown_count = models.iter().filter(|p| is_shown(p)).count();
                let mut all =
                    if known { !models.is_empty() && shown_count == models.len() } else { is_shown(&item.path) };
                let partly = !all && (shown_count > 0 || !known && loaded.iter().any(|l| l.starts_with(&item.path)));
                ui.horizontal(|ui| {
                    let tick = egui::Checkbox::new(&mut all, "").indeterminate(partly);
                    if ui.add_enabled(!known || !models.is_empty(), tick).changed() {
                        let mut models = models.clone();
                        if !known {
                            models = self.models_in(&item.path, inside); // reads it now
                        }
                        models.retain(|p| is_shown(p) != all);
                        if !models.is_empty() {
                            actions.push(if all { Action::Add(models) } else { Action::Remove(models) });
                        }
                    }
                    if ui.link(format!("{icon} {name}")).on_hover_text(item.path.display().to_string()).clicked() {
                        self.set_root(Some(item.path.clone()));
                    }
                    if !models.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(models.len().to_string())
                        });
                    }
                });
            }
            Kind::Model => {
                // in the scene on its own, or with a folder or archive opened whole
                let mut shown = loaded.iter().any(|p| item.path.starts_with(p));
                ui.horizontal(|ui| {
                    let tick = ui.checkbox(&mut shown, name);
                    if tick.on_hover_text(item.path.display().to_string()).changed() {
                        actions.push(if shown {
                            Action::Add(vec![item.path.clone()])
                        } else {
                            Action::Remove(vec![item.path.clone()])
                        });
                    }
                    if let Some(size) = item.size {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(match size {
                                s if s < 1 << 20 => format!("{} KB", s.div_ceil(1024)),
                                s => format!("{:.1} MB", s as f64 / (1u64 << 20) as f64),
                            })
                        });
                    }
                });
            }
        }
    }
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
            // the type comes with the listing; only links need a look at what they point to
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir() || (t.is_symlink() && path.is_dir()));
            let kind = if is_dir {
                Kind::Folder
            } else if sources::is_archive(&path) {
                Kind::Archive
            } else if sources::is_model(&path) {
                Kind::Model
            } else {
                return None;
            };
            let size = (kind == Kind::Model).then(|| entry.metadata().map(|m| m.len()).ok()).flatten();
            Some(Item { path, kind, size })
        })
        .collect();
    sort(&mut items);
    Ok(items)
}

/// The rows of the folder `prefix` (empty = top level) inside `archive`, from its model `entries`.
fn archive_folder(archive: &Path, prefix: &str, entries: &[(String, u64)]) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    for (entry, size) in entries {
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
            let (kind, size) = if deeper { (Kind::Folder, None) } else { (Kind::Model, Some(*size)) };
            items.push(Item { path, kind, size });
        }
    }
    sort(&mut items);
    items
}

/// Folders first, then archives and models, each by name ignoring case.
fn sort(items: &mut [Item]) {
    items.sort_by_key(|i| (i.kind == Kind::Model || i.kind == Kind::Archive, display_name(&i.path).to_lowercase()));
}

/// The drives of the computer (the root folder where there are none).
fn drives() -> Vec<Item> {
    if !cfg!(windows) {
        return vec![folder(Path::new("/"))];
    }
    (b'A'..=b'Z')
        .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
        .filter(|drive| drive.exists())
        .map(|path| folder(&path))
        .collect()
}

/// The row of a folder.
fn folder(path: &Path) -> Item {
    Item { path: path.to_path_buf(), kind: Kind::Folder, size: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_folders_list_one_level() {
        let archive = Path::new("pack.zip");
        let entries = ["a.stl", "inner/b.obj", "inner/deep/c.ply"].map(|n| (n.to_string(), 7));
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
}
