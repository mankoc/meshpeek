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

use crate::sources::{self, Source};
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

/// Everything a file produced, with meshes already on the GPU.
pub struct LoadedFile {
    pub objects: Vec<LoadedObject>,
    pub plates: Vec<Plate>,
    pub skipped: Vec<String>,
}

/// What a loader thread sends back.
pub struct LoadResult {
    /// The file that was loaded.
    pub path: PathBuf,
    /// The loaded file, or why it failed.
    pub result: Result<LoadedFile, String>,
    /// Time taken.
    pub seconds: f64,
    /// The options it was loaded with (a preloaded file is only used while they still apply).
    pub options: LoadOptions,
}

/// Load `source` on a new thread; the result is sent on `done` and `ctx` is asked to repaint.
pub fn spawn(source: Source, options: LoadOptions, device: wgpu::Device, done: Sender<LoadResult>, ctx: egui::Context) {
    run_in_thread(ctx, move || {
        let _ = done.send(load_source(source, options, &device));
    });
}

/// Like [`spawn`] for a path that may name a model inside an archive: the member is unpacked on the worker
/// thread as well, so nothing of the load blocks the UI (used to preload the neighbouring files).
pub fn spawn_path(
    path: PathBuf,
    options: LoadOptions,
    device: wgpu::Device,
    done: Sender<LoadResult>,
    ctx: egui::Context,
) {
    run_in_thread(ctx, move || {
        let start = Instant::now();
        let mut problems = Vec::new();
        let result = match sources::expand([path.clone()], &mut problems).into_iter().next() {
            Some(source) => load_source(source, options, &device),
            None => {
                LoadResult { path, result: Err(problems.join("; ")), seconds: start.elapsed().as_secs_f64(), options }
            }
        };
        let _ = done.send(result);
    });
}

/// Run `work` on a loader thread (big stack), then ask `ctx` to repaint.
fn run_in_thread(ctx: egui::Context, work: impl FnOnce() + Send + 'static) {
    // Big stack: dozens of loads can run at once (e.g. from an archive) and debug builds use a lot of it.
    let spawned = std::thread::Builder::new().stack_size(LOAD_STACK).spawn(move || {
        work();
        ctx.request_repaint();
    });
    if let Err(e) = spawned {
        log::error!("could not start loader thread: {e}");
    }
}

/// Parse `source` and upload its meshes to the GPU (the CPU copies are freed one by one).
fn load_source(source: Source, options: LoadOptions, device: &wgpu::Device) -> LoadResult {
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
                mesh: Arc::new(GpuMesh::upload(device, &obj.mesh, max_buffer)), // obj.mesh is freed after this
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
    LoadResult { path, result, seconds: start.elapsed().as_secs_f64(), options }
}
