//! MeshPeek renderer: uploads core meshes to the GPU and draws them with wgpu into an offscreen texture that
//! the UI shows. No UI code lives here.

pub mod camera;
pub mod gpu_mesh;
pub mod renderer;

pub use camera::Camera;
pub use gpu_mesh::GpuMesh;
pub use renderer::{Material, ObjectId, Palette, Renderer, Style};
pub use wgpu;

/// Colour format of the image handed to the UI (gamma space, as egui expects for native textures).
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
