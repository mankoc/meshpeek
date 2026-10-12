//! Benchmark: load files, upload them to the GPU and render frames while orbiting, off-screen.
//!
//!     meshpeek-bench [--cpu] [--size 1600x1000] [--frames 60] <files...>
//!
//! Prints load time, upload time, median/p90 frame time and process memory (comparable with the Python
//! benchmark in the migration plan).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use meshpeek_core::{LoadOptions, load};
use meshpeek_render::{Camera, GpuMesh, Material, Renderer, Style, wgpu};

fn main() {
    let mut cpu = false;
    let mut size = [1600u32, 1000];
    let mut frames = 60usize;
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cpu" => cpu = true,
            "--size" => {
                let s = args.next().expect("WxH");
                let (w, h) = s.split_once('x').expect("WxH");
                size = [w.parse().expect("width"), h.parse().expect("height")];
            }
            "--frames" => frames = args.next().and_then(|n| n.parse().ok()).expect("number"),
            _ => files.push(PathBuf::from(arg)),
        }
    }
    if files.is_empty() {
        eprintln!("usage: meshpeek-bench [--cpu] [--size WxH] [--frames N] <files...>");
        std::process::exit(2);
    }

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: cpu,
        compatible_surface: None,
    }))
    .expect("no wgpu adapter");
    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() }),
    )
    .expect("device");
    let info = adapter.get_info();
    println!("adapter: {} ({:?}, {:?})", info.name, info.backend, info.device_type);

    let base = memory_mb();
    let mut renderer = Renderer::new(&device, Renderer::best_sample_count(&adapter));
    let plastic = Material { ambient: 0.1, diffuse: 0.8, specular: 0.3, specular_power: 20.0 };
    let (mut load_s, mut upload_s, mut triangles, mut gpu_bytes) = (0.0, 0.0, 0usize, 0u64);
    for file in &files {
        let start = Instant::now();
        let scene = load(file, &LoadOptions::default()).unwrap_or_else(|e| panic!("{e}"));
        load_s += start.elapsed().as_secs_f64();
        let start = Instant::now();
        for obj in scene.objects {
            let mesh = Arc::new(GpuMesh::upload(&device, &obj.mesh, device.limits().max_buffer_size));
            triangles += mesh.triangles;
            gpu_bytes += mesh.gpu_bytes();
            let style = Style { color: [0.76, 0.8, 0.85], material: plastic };
            renderer.add_object(&device, mesh, obj.transform, style, obj.visible);
        } // CPU meshes dropped here
        scene.plates.into_iter().for_each(|p| renderer.add_plate(p));
        upload_s += start.elapsed().as_secs_f64();
    }

    let mut camera = Camera::default();
    let (lo, hi) = renderer.scene_bounds().expect("empty scene");
    camera.frame(lo, hi);
    renderer.render(&device, &queue, size, &camera);
    device.poll(wgpu::PollType::Wait).ok();
    let mut times = Vec::with_capacity(frames);
    for _ in 0..frames {
        camera.orbit(6.0, 0.0);
        let start = Instant::now();
        renderer.render(&device, &queue, size, &camera);
        device.poll(wgpu::PollType::Wait).ok(); // wait for the GPU, like the Python benchmark's WaitForCompletion
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    let pct = |p: f64| times[((times.len() - 1) as f64 * p).round() as usize];
    println!(
        "{:.2}M triangles @{}x{} | load {:.2} s, upload {:.2} s | frame median {:.1} ms, p90 {:.1} ms | GPU buffers {:.0} MB | process memory {:.0} MB (+{:.0} MB)",
        triangles as f64 / 1e6,
        size[0],
        size[1],
        load_s,
        upload_s,
        pct(0.5),
        pct(0.9),
        gpu_bytes as f64 / 1048576.0,
        memory_mb(),
        memory_mb() - base
    );
}

/// Physical memory used by the process, in MiB.
fn memory_mb() -> f64 {
    memory_stats::memory_stats().map_or(0.0, |m| m.physical_mem as f64 / (1024.0 * 1024.0))
}
