use std::path::PathBuf;

use glam::DMat4;

use crate::mesh::Mesh;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Model,
    Support,
}

#[derive(Debug, Clone)]
pub struct SceneObject {
    pub name: String,
    pub mesh: Mesh,
    /// Object -> world.
    pub transform: DMat4,
    pub kind: ObjectKind,
    pub visible: bool,
    pub source: PathBuf,
    /// Index into [`Scene::objects`] of the object this one belongs to (e.g. a model's supports).
    pub parent: Option<usize>,
    /// Extra facts about the source file (format, size, comment…) as label/value pairs.
    pub info: Vec<(String, String)>,
}

impl SceneObject {
    pub fn new(name: impl Into<String>, mesh: Mesh, source: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            mesh,
            transform: DMat4::IDENTITY,
            kind: ObjectKind::Model,
            visible: true,
            source: source.into(),
            parent: None,
            info: Vec::new(),
        }
    }
}

/// Build volume: size (x, y, z) in mm, centred on `position` in XY, standing on its Z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plate {
    pub size: [f64; 3],
    pub position: [f64; 3],
}

#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub objects: Vec<SceneObject>,
    pub plates: Vec<Plate>,
    /// `"<name>: <reason>"` for objects that could not be loaded.
    pub skipped: Vec<String>,
}
