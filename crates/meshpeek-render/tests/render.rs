//! Headless rendering tests. They need a wgpu adapter (any GPU, or a software one such as WARP/lavapipe)
//! and skip with a message when none is available.
//!
//! Set `MESHPEEK_RENDER_OUT=<dir>` to also save the rendered images as PNG for a visual check.

use std::path::Path;
use std::sync::Arc;

use glam::DVec3;
use meshpeek_core::{LoadOptions, Mesh, ObjectKind, load};
use meshpeek_render::{Camera, GpuMesh, Material, Renderer, Style};

const PLASTIC: Material = Material { ambient: 0.1, diffuse: 0.8, specular: 0.3, specular_power: 20.0 };

fn device() -> Option<(wgpu::Device, wgpu::Queue, wgpu::Adapter)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .or_else(|_| {
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    force_fallback_adapter: true,
                    ..Default::default()
                }))
            })
            .ok()?;
    let features = adapter.features() & wgpu::Features::POLYGON_MODE_LINE;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .ok()?;
    Some((device, queue, adapter))
}

fn save(name: &str, pixels: &[u8], size: [u32; 2]) {
    if let Ok(dir) = std::env::var("MESHPEEK_RENDER_OUT") {
        image::save_buffer(Path::new(&dir).join(name), pixels, size[0], size[1], image::ColorType::Rgba8).unwrap();
    }
}

fn cube() -> Mesh {
    let corners: Vec<[f32; 3]> =
        (0..8).map(|i| [(i & 1) as f32 * 10.0, ((i >> 1) & 1) as f32 * 10.0, ((i >> 2) & 1) as f32 * 10.0]).collect();
    let faces = vec![
        [0, 2, 1],
        [1, 2, 3],
        [4, 5, 6],
        [5, 7, 6],
        [0, 1, 4],
        [1, 5, 4],
        [2, 6, 3],
        [3, 6, 7],
        [0, 4, 2],
        [2, 4, 6],
        [1, 3, 5],
        [3, 7, 5],
    ];
    Mesh::new(corners, faces)
}

#[test]
fn renders_a_cube_over_the_background() {
    let Some((device, queue, adapter)) = device() else {
        eprintln!("skipping: no wgpu adapter");
        return;
    };
    eprintln!("adapter: {:?}", adapter.get_info().name);
    let mut renderer = Renderer::new(&device, Renderer::best_sample_count(&adapter));
    let mesh = Arc::new(GpuMesh::upload(&device, &cube(), device.limits().max_buffer_size));
    let style = Style { color: [0.76, 0.8, 0.85], material: PLASTIC };
    renderer.add_object(&device, mesh, glam::DMat4::IDENTITY, style, true);
    let mut camera = Camera::default();
    let (lo, hi) = renderer.scene_bounds().unwrap();
    camera.frame(lo, hi);
    renderer.render(&device, &queue, [200, 150], &camera);
    let (pixels, size) = renderer.read_pixels(&device, &queue).unwrap();
    save("cube.png", &pixels, size);

    let at = |x: u32, y: u32| &pixels[((y * size[0] + x) * 4) as usize..][..3];
    let corner = at(2, 2);
    assert!(corner[0] > 50 && corner[0] < 100, "background gradient expected, got {corner:?}");
    let centre_pixels = (50..100).flat_map(|y| (60..140).map(move |x| (x, y)));
    // Shading matches VTK's light kit: lit top (~158), side faces only a little brighter than the background.
    let covered = centre_pixels.filter(|&(x, y)| at(x, y).iter().zip(corner).any(|(a, b)| a.abs_diff(*b) > 10)).count();
    assert!(covered > 50 * 80 / 2, "cube should cover the middle of the image ({covered} pixels differ)");
    assert!(at(100, 40)[0] > 140, "top face should be lit: {:?}", at(100, 40));
}

#[test]
fn renders_the_bust_sample_with_supports() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Favorite_BUST - Lychee.lys");
    if std::fs::read(&file).map_or(true, |b| b.starts_with(b"version https://git-lfs")) {
        eprintln!("skipping: sample not available");
        return;
    }
    let Some((device, queue, adapter)) = device() else {
        eprintln!("skipping: no wgpu adapter");
        return;
    };
    let scene = load(&file, &LoadOptions::default()).unwrap();
    let mut renderer = Renderer::new(&device, Renderer::best_sample_count(&adapter));
    for obj in &scene.objects {
        let color = if obj.kind == ObjectKind::Support { [0.91, 0.635, 0.29] } else { [0.76, 0.8, 0.85] };
        let mesh = Arc::new(GpuMesh::upload(&device, &obj.mesh, device.limits().max_buffer_size));
        renderer.add_object(&device, mesh, obj.transform, Style { color, material: PLASTIC }, obj.visible);
    }
    scene.plates.iter().for_each(|p| renderer.add_plate(*p));
    let mut camera = Camera::default();
    camera.view_from(DVec3::new(-0.3, -1.0, 0.55), DVec3::Z, 1.0);
    let (lo, hi) = renderer.scene_bounds().unwrap();
    camera.frame(lo, hi);
    renderer.render(&device, &queue, [1200, 750], &camera);
    let (pixels, size) = renderer.read_pixels(&device, &queue).unwrap();
    save("bust.png", &pixels, size);
    let orange = pixels.chunks(4).filter(|p| p[0] > 150 && p[1] > 90 && p[1] < 190 && p[2] < 110).count();
    assert!(orange > 1000, "supports not visible ({orange} orange pixels)");
}
