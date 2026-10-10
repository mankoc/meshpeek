//! Loading any supported file into a [`Scene`].

pub mod lys;
pub mod obj;
pub mod ply;
pub mod stl;

use std::fs::File;
use std::path::Path;

use glam::{DMat4, DVec4};
use memmap2::Mmap;

use crate::error::{LoadError, Result};
use crate::mesh::Mesh;
use crate::scene::{Scene, SceneObject};

/// Lower-case extensions (without the dot) that [`load`] understands.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["stl", "obj", "ply", "lys"];

/// Rotation of +90 degrees about X: a Y-up model (x, y, z) becomes Z-up (x, -z, y).
pub const Y_UP_TO_Z_UP: DMat4 = DMat4::from_cols(
    DVec4::new(1.0, 0.0, 0.0, 0.0),
    DVec4::new(0.0, 0.0, 1.0, 0.0),
    DVec4::new(0.0, -1.0, 0.0, 0.0),
    DVec4::new(0.0, 0.0, 0.0, 1.0),
);

/// Options that change what a load produces (changing them needs a reload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadOptions {
    /// Facets around `.lys` support columns and their joint spheres.
    pub support_sides: u32,
    /// OBJ files use Y as up (Blender, Maya, ZBrush): rotate them to this viewer's Z-up.
    pub obj_y_up: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self { support_sides: 12, obj_y_up: true }
    }
}

/// Lower-case extension of `path`, without the dot.
pub fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Load a single-mesh file (STL, OBJ, PLY).
pub fn load_mesh(path: &Path) -> Result<Mesh> {
    match extension(path).as_str() {
        "stl" => stl::load(path),
        "obj" => obj::load(path),
        "ply" => ply::load(path),
        other => Err(LoadError::Unsupported(format!(".{other}"))),
    }
}

/// Load any supported file. Single meshes keep their file coordinates, apart from the OBJ up-axis fix.
pub fn load(path: &Path, options: &LoadOptions) -> Result<Scene> {
    let ext = extension(path);
    if ext == "lys" {
        return lys::load(path, options.support_sides);
    }
    let mut object = SceneObject::new(stem(path), load_mesh(path)?, path);
    if ext == "obj" && options.obj_y_up {
        object.transform = Y_UP_TO_Z_UP;
    }
    Ok(Scene { objects: vec![object], ..Scene::default() })
}

pub(crate) fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Memory-map a file for reading: the OS pages it in on demand instead of us copying it.
pub(crate) fn map_file(path: &Path) -> Result<Mmap> {
    let file = File::open(path).map_err(|e| LoadError::io(path, e))?;
    if file.metadata().map_err(|e| LoadError::io(path, e))?.len() == 0 {
        return Err(LoadError::format(format!("{}: file is empty", path.display())));
    }
    // SAFETY: the map is read-only; if another program truncates the file while we read it the process
    // can fault, the usual trade-off of mapped I/O, accepted here for speed and memory.
    unsafe { Mmap::map(&file) }.map_err(|e| LoadError::io(path, e))
}

/// Split `data` into about `parts` slices that end on line boundaries (for parallel parsing).
pub(crate) fn line_chunks(data: &[u8], parts: usize) -> Vec<&[u8]> {
    let target = (data.len() / parts.max(1)).max(1 << 16);
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < data.len() {
        let mut end = (start + target).min(data.len());
        if end < data.len() {
            end = memchr::memchr(b'\n', &data[end..]).map_or(data.len(), |i| end + i + 1);
        }
        chunks.push(&data[start..end]);
        start = end;
    }
    chunks
}

/// Fan-triangulate a polygon given by its corner indices.
pub(crate) fn fan(corners: &[u32], out: &mut Vec<[u32; 3]>) {
    for k in 1..corners.len().saturating_sub(1) {
        out.push([corners[0], corners[k], corners[k + 1]]);
    }
}
