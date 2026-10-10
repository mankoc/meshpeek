//! Background loading: parse a file and upload its meshes to the GPU on a worker thread, so the UI never
//! waits. The CPU copies of the meshes are dropped once they are on the GPU.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Instant;

use eframe::egui;
use glam::DMat4;
use meshpeek_core::io::{Data, load_data};
use meshpeek_core::{LoadOptions, ObjectKind, Plate, load};

use crate::sources::Source;
use meshpeek_render::{GpuMesh, wgpu};

/// Stack size of loader and parser threads.
pub const LOAD_STACK: usize = 64 << 20;

/// One object of a loaded file, ready to be added to the renderer.
pub struct LoadedObject {
    pub name: String,
    pub kind: ObjectKind,
    pub parent: Option<usize>, // index within the same file
    pub visible: bool,
    pub transform: DMat4,
    pub info: Vec<(String, String)>,
    pub mesh: Arc<GpuMesh>,
}

pub struct LoadedFile {
    pub objects: Vec<LoadedObject>,
    pub plates: Vec<Plate>,
    pub skipped: Vec<String>,
}

pub struct LoadResult {
    pub path: PathBuf,
    pub result: Result<LoadedFile, String>,
    pub seconds: f64,
}

/// Load `path` on a new thread; the result is sent on `done` and `ctx` is asked to repaint.
pub fn spawn(source: Source, options: LoadOptions, device: wgpu::Device, done: Sender<LoadResult>, ctx: egui::Context) {
    // Big stack: dozens of loads can run at once (e.g. from an archive) and debug builds use a lot of it.
    let spawned = std::thread::Builder::new().stack_size(LOAD_STACK).spawn(move || {
        let start = Instant::now();
        let (path, loaded) = match source {
            Source::Disk(path) => {
                let scene = load(&path, &options);
                (path, scene)
            }
            Source::Memory { name, data } => {
                let scene = load_data(&name, Data::Owned(data), &options);
                (name, scene)
            }
        };
        let result = loaded.map_err(|e| e.to_string()).map(|scene| {
            let max_buffer = device.limits().max_buffer_size;
            let objects = scene
                .objects
                .into_iter()
                .map(|obj| LoadedObject {
                    mesh: Arc::new(GpuMesh::upload(&device, &obj.mesh, max_buffer)), // obj.mesh is freed after this
                    name: obj.name,
                    kind: obj.kind,
                    parent: obj.parent,
                    visible: obj.visible,
                    transform: obj.transform,
                    info: obj.info,
                })
                .collect();
            LoadedFile { objects, plates: scene.plates, skipped: scene.skipped }
        });
        let _ = done.send(LoadResult { path, result, seconds: start.elapsed().as_secs_f64() });
        ctx.request_repaint();
    });
    if let Err(e) = spawned {
        log::error!("could not start loader thread: {e}");
    }
}
