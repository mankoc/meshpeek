//! Lychee Slicer `.lys` scenes. Format notes: `LYS_FILE_FORMAT.md` at the repository root.

pub mod codec;
pub mod container;
pub mod loader;
pub mod mesh_bin;
pub mod supports;
pub mod value;

pub use loader::load;
