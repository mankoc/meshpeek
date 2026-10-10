//! MeshPeek core: file formats, meshes and scenes. No GPU or UI code lives here.
//!
//! Entry point: [`io::load`] turns any supported file into a [`scene::Scene`].

pub mod error;
pub mod io;
pub mod mesh;
pub mod scene;

pub use error::{LoadError, Result};
pub use io::{LoadOptions, SUPPORTED_EXTENSIONS, load, load_mesh};
pub use mesh::Mesh;
pub use scene::{ObjectKind, Plate, Scene, SceneObject};
