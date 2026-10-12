//! User preferences and recent files, stored as TOML in the OS config directory.

use std::path::{Path, PathBuf};

use meshpeek_core::LoadOptions;
use meshpeek_render::Material;
use serde::{Deserialize, Serialize};

/// Material presets (same values as the Python version).
pub const MATERIALS: [(&str, Material); 4] = [
    ("Matte", Material { ambient: 0.15, diffuse: 0.9, specular: 0.0, specular_power: 1.0 }),
    ("Plastic", Material { ambient: 0.1, diffuse: 0.8, specular: 0.3, specular_power: 20.0 }),
    ("Glossy", Material { ambient: 0.1, diffuse: 0.7, specular: 1.0, specular_power: 10.0 }),
    ("Metal", Material { ambient: 0.3, diffuse: 0.45, specular: 1.0, specular_power: 12.0 }),
];

/// Facets around `.lys` support columns and their joint spheres.
pub const SUPPORT_DETAIL: [(&str, u32); 4] = [("Low", 6), ("Medium", 12), ("High", 24), ("Very high", 48)];

/// Entries kept in File > Open Recent.
pub const MAX_RECENT: usize = 10;

/// Mouse orbit styles: turn around the world's vertical axis, or around the current view's axes.
pub const ORBIT_STYLES: [&str; 2] = ["Turntable", "Trackball"];

/// UI theme choices; "System" follows the operating system.
pub const THEMES: [&str; 3] = ["System", "Light", "Dark"];

/// Space mouse behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpaceMouseSettings {
    pub enabled: bool,
    /// Speed multiplier (1 = default).
    pub sensitivity: f32,
    pub invert_pan: bool,
    pub invert_zoom: bool,
    pub invert_rotation: bool,
}

impl Default for SpaceMouseSettings {
    fn default() -> Self {
        Self { enabled: true, sensitivity: 1.0, invert_pan: false, invert_zoom: false, invert_rotation: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
/// Preferences, saved as TOML in the user's configuration folder.
pub struct Settings {
    /// Name of a [`MATERIALS`] entry for models.
    pub model_material: String,
    /// Used when the file has no colours of its own.
    pub model_color: [u8; 3],
    /// Name of a [`MATERIALS`] entry for supports.
    pub support_material: String,
    /// Colour of supports and rafts.
    pub support_color: [u8; 3],
    /// Facets round support columns (a name in [`SUPPORT_DETAIL`]).
    pub support_detail: String,
    /// Blender, Maya and ZBrush export OBJ with Y up.
    pub obj_y_up: bool,
    /// UI theme: one of [`THEMES`].
    pub theme: String,
    /// Draw the build plate of `.lys` files.
    pub show_plate: bool,
    /// Draw the printer's build volume (the box above the plate) of `.lys` files.
    pub show_volume: bool,
    /// Toolbar buttons show a text label under their icon.
    pub toolbar_text: bool,
    /// Load the previous and next file of the folder in the background, for the arrow keys.
    pub preload_neighbours: bool,
    /// One of [`ORBIT_STYLES`].
    pub mouse_orbit: String,
    /// Space mouse options.
    pub space_mouse: SpaceMouseSettings,
    /// Recently opened files, newest first.
    pub recent_files: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model_material: "Plastic".into(),
            model_color: [0xc3, 0xcc, 0xd9],
            support_material: "Plastic".into(),
            support_color: [0xe8, 0xa2, 0x4a],
            support_detail: "Medium".into(),
            obj_y_up: true,
            theme: "System".into(),
            show_plate: true,
            show_volume: true,
            toolbar_text: true,
            preload_neighbours: true,
            mouse_orbit: "Turntable".into(),
            space_mouse: SpaceMouseSettings::default(),
            recent_files: Vec::new(),
        }
    }
}

impl Settings {
    /// Settings file: `MESHPEEK_CONFIG_DIR` if set (tests), otherwise the OS config directory.
    pub fn file() -> Option<PathBuf> {
        let dir = match std::env::var_os("MESHPEEK_CONFIG_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => directories::ProjectDirs::from("", "", "MeshPeek")?.config_dir().to_path_buf(),
        };
        Some(dir.join("settings.toml"))
    }

    /// Load the saved settings; anything missing or invalid falls back to the default.
    pub fn load() -> Self {
        let text = Self::file().and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
        Self::from_toml(&text)
    }

    /// Parse saved settings; unknown or invalid values fall back to the defaults.
    pub fn from_toml(text: &str) -> Self {
        let mut settings: Self = toml::from_str(text).unwrap_or_default();
        let defaults = Self::default();
        if material(&settings.model_material).is_none() {
            settings.model_material = defaults.model_material.clone();
        }
        if material(&settings.support_material).is_none() {
            settings.support_material = defaults.support_material.clone();
        }
        if !THEMES.contains(&settings.theme.as_str()) {
            settings.theme = defaults.theme.clone();
        }
        if !ORBIT_STYLES.contains(&settings.mouse_orbit.as_str()) {
            settings.mouse_orbit = defaults.mouse_orbit.clone();
        }
        settings.space_mouse.sensitivity = settings.space_mouse.sensitivity.clamp(0.1, 5.0);
        if !SUPPORT_DETAIL.iter().any(|(name, _)| *name == settings.support_detail) {
            settings.support_detail = defaults.support_detail;
        }
        settings.recent_files.truncate(MAX_RECENT);
        settings
    }

    /// Write the settings file (errors are logged, not reported).
    pub fn save(&self) {
        let Some(file) = Self::file() else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&file, toml::to_string_pretty(self).unwrap_or_default()) {
            log::warn!("could not save settings to {}: {e}", file.display());
        }
    }

    /// The part of the settings that changes how files are loaded (changing it needs a reload).
    pub fn load_options(&self) -> LoadOptions {
        let sides = SUPPORT_DETAIL.iter().find(|(n, _)| *n == self.support_detail).map_or(12, |(_, s)| *s);
        LoadOptions { support_sides: sides, obj_y_up: self.obj_y_up }
    }

    /// Put `path` first in the recent-files list (no duplicates, at most [`MAX_RECENT`]).
    pub fn add_recent(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let path = PathBuf::from(path.to_string_lossy().trim_start_matches(r"\\?\")); // no Windows verbatim prefix
        self.recent_files.retain(|p| !same_path(p, &path));
        self.recent_files.insert(0, path);
        self.recent_files.truncate(MAX_RECENT);
    }
}

/// The material called `name` in [`MATERIALS`].
pub fn material(name: &str) -> Option<Material> {
    MATERIALS.iter().find(|(n, _)| *n == name).map(|(_, m)| *m)
}

/// An 8-bit colour as 0..1 floats.
pub fn rgb(color: [u8; 3]) -> [f32; 3] {
    color.map(|c| c as f32 / 255.0)
}

/// Paths are equal (ignoring case on Windows).
fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) { a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()) } else { a == b }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_validation() {
        let settings = Settings { model_material: "Metal".into(), model_color: [1, 2, 3], ..Settings::default() };
        let text = toml::to_string(&settings).unwrap();
        assert_eq!(Settings::from_toml(&text), settings);
        let invalid = Settings::from_toml(
            "support_material = \"Chrome\"\nsupport_detail = \"Ultra\"\ntheme = \"Neon\"\nmouse_orbit = \"Spin\"\n",
        );
        assert_eq!(invalid.support_material, "Plastic");
        assert_eq!(invalid.support_detail, "Medium");
        assert_eq!(invalid.theme, "System");
        assert_eq!(invalid.mouse_orbit, "Turntable");
        let spacemouse = Settings::from_toml("[space_mouse]\nsensitivity = 50.0\ninvert_zoom = true\n");
        assert_eq!(spacemouse.space_mouse.sensitivity, 5.0); // clamped
        assert!(spacemouse.space_mouse.invert_zoom && spacemouse.space_mouse.enabled);
    }

    #[test]
    fn recent_files_newest_first_without_duplicates_and_capped() {
        let mut settings = Settings::default();
        for i in 0..MAX_RECENT + 3 {
            settings.add_recent(Path::new(&format!("model{i}.stl")));
        }
        settings.add_recent(Path::new("model5.stl"));
        assert_eq!(settings.recent_files.len(), MAX_RECENT);
        assert!(settings.recent_files[0].ends_with("model5.stl"));
        assert_eq!(settings.recent_files.iter().filter(|p| p.ends_with("model5.stl")).count(), 1);
    }

    #[test]
    fn load_options_follow_detail_and_up_axis() {
        let settings = Settings { support_detail: "Very high".into(), obj_y_up: false, ..Settings::default() };
        assert_eq!(settings.load_options(), LoadOptions { support_sides: 48, obj_y_up: false });
    }
}
