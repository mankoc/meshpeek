//! User preferences and recent files, stored as TOML in the OS config directory.

use std::path::{Path, PathBuf};

use meshpeek_core::LoadOptions;
use meshpeek_render::Material;
use serde::{Deserialize, Serialize};

/// The materials along the reflectiveness scale, evenly spaced from 0 to 1 (the presets of earlier versions, with
/// the values of the Python version): matte, plastic, glossy, metal. Values in between are blended.
const MATERIAL_SCALE: [(&str, Material); 4] = [
    ("Matte", Material { ambient: 0.15, diffuse: 0.9, specular: 0.0, specular_power: 1.0 }),
    ("Plastic", Material { ambient: 0.1, diffuse: 0.8, specular: 0.3, specular_power: 20.0 }),
    ("Glossy", Material { ambient: 0.1, diffuse: 0.7, specular: 1.0, specular_power: 10.0 }),
    ("Metal", Material { ambient: 0.3, diffuse: 0.45, specular: 1.0, specular_power: 12.0 }),
];

/// Default reflectiveness of models and supports: plastic.
const PLASTIC: f32 = 1.0 / 3.0;

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
    /// How reflective models are, from 0 (matte) to 1 (metal); see [`material`].
    pub model_reflectiveness: f32,
    /// Used when the file has no colours of its own.
    pub model_color: [u8; 3],
    /// How reflective supports are, from 0 (matte) to 1 (metal).
    pub support_reflectiveness: f32,
    /// Colour of supports and rafts.
    pub support_color: [u8; 3],
    /// Facets round support columns (a name in [`SUPPORT_DETAIL`]).
    pub support_detail: String,
    /// Blender, Maya and ZBrush export OBJ with Y up.
    pub obj_y_up: bool,
    /// UI theme: one of [`THEMES`].
    pub theme: String,
    /// Fill the 3D view's background with [`Settings::background_color`] instead of the theme's gradient.
    pub custom_background: bool,
    /// The background colour used with `custom_background` (kept while it is off).
    pub background_color: [u8; 3],
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
            model_reflectiveness: PLASTIC,
            model_color: [0xc3, 0xcc, 0xd9],
            support_reflectiveness: PLASTIC,
            support_color: [0xe8, 0xa2, 0x4a],
            support_detail: "Medium".into(),
            obj_y_up: true,
            theme: "System".into(),
            custom_background: false,
            background_color: [0x30, 0x33, 0x38],
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
        // earlier versions saved a material preset by name: start the slider where that preset sits
        let saved: toml::Table = text.parse().unwrap_or_default();
        let preset = |key: &str| {
            let name = saved.get(key)?.as_str()?;
            let at = MATERIAL_SCALE.iter().position(|(n, _)| *n == name)?;
            Some(at as f32 / (MATERIAL_SCALE.len() - 1) as f32)
        };
        if !saved.contains_key("model_reflectiveness") {
            settings.model_reflectiveness = preset("model_material").unwrap_or(defaults.model_reflectiveness);
        }
        if !saved.contains_key("support_reflectiveness") {
            settings.support_reflectiveness = preset("support_material").unwrap_or(defaults.support_reflectiveness);
        }
        for (value, default) in [
            (&mut settings.model_reflectiveness, defaults.model_reflectiveness),
            (&mut settings.support_reflectiveness, defaults.support_reflectiveness),
        ] {
            *value = if value.is_finite() { value.clamp(0.0, 1.0) } else { default };
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
        // written next to it, then swapped in: a crash or full disk mid-write cannot leave half a file
        let partial = file.with_extension("toml.partial");
        let saved = std::fs::write(&partial, toml::to_string_pretty(self).unwrap_or_default())
            .and_then(|()| std::fs::rename(&partial, &file));
        if let Err(e) = saved {
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

/// The material of `reflectiveness` (0 matte, 1 metal): blended between the two neighbouring materials of the
/// scale, so the look changes smoothly as the slider moves.
pub fn material(reflectiveness: f32) -> Material {
    let last = (MATERIAL_SCALE.len() - 1) as f32;
    let at = if reflectiveness.is_finite() { reflectiveness.clamp(0.0, 1.0) * last } else { PLASTIC * last };
    let below = (at.floor() as usize).min(MATERIAL_SCALE.len() - 2);
    let t = at - below as f32;
    let (a, b) = (MATERIAL_SCALE[below].1, MATERIAL_SCALE[below + 1].1);
    let mix = |x: f32, y: f32| x + (y - x) * t;
    Material {
        ambient: mix(a.ambient, b.ambient),
        diffuse: mix(a.diffuse, b.diffuse),
        specular: mix(a.specular, b.specular),
        specular_power: mix(a.specular_power, b.specular_power),
    }
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
        let settings = Settings {
            model_reflectiveness: 0.8,
            model_color: [1, 2, 3],
            custom_background: true,
            background_color: [255, 255, 255],
            ..Settings::default()
        };
        let text = toml::to_string(&settings).unwrap();
        assert_eq!(Settings::from_toml(&text), settings);
        let invalid = Settings::from_toml(
            "support_reflectiveness = 7.0\nsupport_detail = \"Ultra\"\ntheme = \"Neon\"\nmouse_orbit = \"Spin\"\n",
        );
        assert_eq!(invalid.support_reflectiveness, 1.0); // clamped
        assert_eq!(invalid.support_detail, "Medium");
        assert_eq!(invalid.theme, "System");
        assert_eq!(invalid.mouse_orbit, "Turntable");
        let spacemouse = Settings::from_toml("[space_mouse]\nsensitivity = 50.0\ninvert_zoom = true\n");
        assert_eq!(spacemouse.space_mouse.sensitivity, 5.0); // clamped
        assert!(spacemouse.space_mouse.invert_zoom && spacemouse.space_mouse.enabled);
    }

    #[test]
    fn material_presets_of_earlier_versions_become_reflectiveness() {
        let old = Settings::from_toml("model_material = \"Metal\"\nsupport_material = \"Matte\"\n");
        assert_eq!((old.model_reflectiveness, old.support_reflectiveness), (1.0, 0.0));
        let unknown = Settings::from_toml("model_material = \"Chrome\"\n");
        assert_eq!(unknown.model_reflectiveness, PLASTIC);
        // a reflectiveness saved by this version wins over an old preset name
        let both = Settings::from_toml("model_material = \"Metal\"\nmodel_reflectiveness = 0.25\n");
        assert_eq!(both.model_reflectiveness, 0.25);
    }

    #[test]
    fn the_scale_passes_through_the_presets_and_blends_between_them() {
        for (i, (_, preset)) in MATERIAL_SCALE.iter().enumerate() {
            assert_eq!(material(i as f32 / 3.0), *preset);
        }
        let half = material(1.0 / 6.0); // halfway from matte to plastic
        assert!((half.specular - 0.15).abs() < 1e-6 && (half.diffuse - 0.85).abs() < 1e-6);
        assert_eq!(material(f32::NAN), material(PLASTIC));
        assert_eq!(material(-1.0), MATERIAL_SCALE[0].1);
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
