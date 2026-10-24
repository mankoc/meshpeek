//! Background loading: parse a file and upload its meshes to the GPU on a worker thread, so the UI never
//! waits. The CPU copies of the meshes are dropped once they are on the GPU.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use eframe::egui;
use glam::DMat4;
use meshpeek_core::io::{Data, load_data};
use meshpeek_core::{LoadOptions, ObjectKind, Plate, load};

use crate::sources::{self, Source};
use meshpeek_render::{GpuMesh, wgpu};

/// Stack size of loader and parser threads.
pub const LOAD_STACK: usize = 64 << 20;

/// Files parsed at the same time. Each parse already uses every core, so more would only add memory (every
/// file's CPU copy at once) and, on a hard disk, seeks between the files being read.
const LOAD_SLOTS: usize = 2;

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
    /// The last result of its [`spawn`] (a folder sends one per model file).
    pub last: bool,
}

/// Load `path` in the background: a model file, a model inside an archive (unpacked on the worker as well) or a
/// folder (every model file below it). Each file's result is sent on `done`, the last one marked
/// [`LoadResult::last`], and `ctx` is asked to repaint.
pub fn spawn(path: PathBuf, options: LoadOptions, device: wgpu::Device, done: Sender<LoadResult>, ctx: egui::Context) {
    run_in_thread(ctx.clone(), move || {
        let start = Instant::now();
        let mut problems = Vec::new();
        let found = sources::expand([path.clone()], &mut problems);
        if found.is_empty() {
            let seconds = start.elapsed().as_secs_f64();
            let _ = done.send(LoadResult { path, result: Err(problems.join("; ")), seconds, options, last: true });
            return;
        }
        // a few workers take the files in turn; the one finishing the last file marks its result
        let remaining = AtomicUsize::new(found.len());
        let queue = Mutex::new(found.into_iter());
        std::thread::scope(|scope| {
            for _ in 0..LOAD_SLOTS.min(remaining.load(Ordering::Relaxed)) {
                let worker = || loop {
                    let next = queue.lock().unwrap().next(); // not in the `while let`: that would hold the lock
                    let Some(source) = next else { break };
                    let mut result = {
                        let _slot = SLOTS.take();
                        load_source(source, options, &device)
                    };
                    // every earlier result was sent before its worker counted down, so this one comes last
                    result.last = remaining.fetch_sub(1, Ordering::AcqRel) == 1;
                    let _ = done.send(result);
                    ctx.request_repaint();
                };
                let spawned = std::thread::Builder::new().stack_size(LOAD_STACK).spawn_scoped(scope, worker);
                if let Err(e) = spawned {
                    log::error!("could not start loader thread: {e}");
                }
            }
        });
    });
}

/// Run `work` on a loader thread (big stack), then ask `ctx` to repaint.
fn run_in_thread(ctx: egui::Context, work: impl FnOnce() + Send + 'static) {
    // Big stack: the parsers recurse, and debug builds use a lot of it.
    let spawned = std::thread::Builder::new().stack_size(LOAD_STACK).spawn(move || {
        work();
        ctx.request_repaint();
    });
    if let Err(e) = spawned {
        log::error!("could not start loader thread: {e}");
    }
}

/// The [`LOAD_SLOTS`] shared by every load of the app.
static SLOTS: Slots = Slots { free: Mutex::new(LOAD_SLOTS), freed: Condvar::new() };

/// A counting semaphore: [`Slots::take`] waits for a free slot and holds it until dropped.
struct Slots {
    free: Mutex<usize>,
    freed: Condvar,
}

/// A slot taken from [`SLOTS`], given back on drop.
struct Slot;

impl Slots {
    fn take(&'static self) -> Slot {
        let mut free = self.free.lock().unwrap();
        while *free == 0 {
            free = self.freed.wait(free).unwrap();
        }
        *free -= 1;
        Slot
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        *SLOTS.free.lock().unwrap() += 1;
        SLOTS.freed.notify_one();
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
    LoadResult { path, result, seconds: start.elapsed().as_secs_f64(), options, last: false }
}
