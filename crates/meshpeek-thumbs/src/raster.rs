//! Draws a scene into a small picture on the CPU: no graphics device is started inside Explorer's helper process.

use glam::{DMat3, DMat4, DVec3, Vec2, Vec3};
use meshpeek_core::{ObjectKind, Scene};
use rayon::prelude::*;

use crate::Picture;

/// Samples per pixel in each direction.
const SAMPLES: usize = 2;
/// The part of the picture the scene fills.
const FILL: f32 = 0.94;
/// The app's default colours.
const MODEL: [f32; 3] = [195.0, 204.0, 217.0];
const SUPPORT: [f32; 3] = [232.0, 162.0, 74.0];

/// The visible objects of `scene` from the app's isometric direction, on a transparent background; `None` when
/// there is nothing to draw.
pub fn draw(scene: &Scene, size: u32) -> Option<Picture> {
    let towards = DVec3::ONE.normalize(); // from the scene to the eye
    let right = DVec3::Z.cross(towards).normalize();
    let view = DMat4::from_mat3(DMat3::from_cols(right, towards.cross(right), towards).transpose());
    let objects: Vec<_> = scene
        .objects
        .iter()
        .filter(|o| o.visible)
        .map(|o| {
            let place = view * o.transform;
            let see = |v: &[f32; 3]| place.transform_point3(DVec3::from_array(v.map(f64::from))).as_vec3();
            let seen: Vec<Vec3> = o.mesh.vertices.par_iter().map(see).collect();
            (o, seen)
        })
        .collect();
    let (low, high) = objects
        .iter()
        .flat_map(|(o, seen)| o.mesh.faces.iter().flatten().map(|&i| seen[i as usize]))
        .filter(|v| v.is_finite())
        .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(low, high), v| (low.min(v), high.max(v)));
    let extent = (high - low).truncate().max_element();
    if !(extent.is_finite() && extent > 0.0) {
        return None;
    }
    let width = size as usize * SAMPLES;
    let scale = width as f32 * FILL / extent;
    let middle = (low + high).truncate() / 2.0;
    let half = width as f32 / 2.0;
    // a view-space point in samples, y down
    let screen = |v: Vec3| Vec3::new((v.x - middle.x) * scale + half, half - (v.y - middle.y) * scale, v.z);
    let mut depth = vec![f32::NEG_INFINITY; width * width];
    let mut colour = vec![[0u8; 3]; width * width];
    let light = Vec3::new(-0.35, 0.55, 0.76).normalize(); // from above left, in view space
    for (object, seen) in &objects {
        let plain = if object.kind == ObjectKind::Support { SUPPORT } else { MODEL };
        for face in &object.mesh.faces {
            let corners = face.map(|i| seen[i as usize]);
            let [a, b, c] = corners;
            let normal = (b - a).cross(c - a).normalize_or_zero();
            if normal == Vec3::ZERO || !corners.iter().all(|v| v.is_finite()) {
                continue;
            }
            let normal = if normal.z < 0.0 { -normal } else { normal }; // both sides are lit
            let shade = 0.28 + 0.72 * normal.dot(light).max(0.0);
            let base = match &object.mesh.colors {
                Some(colors) => {
                    [0, 1, 2].map(|k| face.iter().map(|&i| colors[i as usize][k] as f32).sum::<f32>() / 3.0)
                }
                None => plain,
            };
            let lit = base.map(|v| (v * shade).min(255.0) as u8);
            fill(corners.map(screen), width, |at, z| {
                if z > depth[at] {
                    depth[at] = z;
                    colour[at] = lit;
                }
            });
        }
    }
    let per_pixel = (SAMPLES * SAMPLES) as u32;
    let mut pixels = Vec::with_capacity((size * size) as usize);
    for y in 0..size as usize {
        for x in 0..size as usize {
            let (mut sum, mut covered) = ([0u32; 3], 0u32);
            for s in 0..SAMPLES * SAMPLES {
                let at = (y * SAMPLES + s / SAMPLES) * width + x * SAMPLES + s % SAMPLES;
                if depth[at] > f32::NEG_INFINITY {
                    (0..3).for_each(|k| sum[k] += colour[at][k] as u32);
                    covered += 1;
                }
            }
            let [r, g, b] = sum.map(|v| (v / per_pixel) as u8); // premultiplied by the coverage
            pixels.push([r, g, b, (covered * 255 / per_pixel) as u8]);
        }
    }
    Some(Picture { width: size, height: size, pixels })
}

/// Call `put(index, depth)` for each sample whose centre lies in the triangle.
fn fill([a, b, c]: [Vec3; 3], width: usize, mut put: impl FnMut(usize, f32)) {
    let edge = |p: Vec2, q: Vec2, r: Vec2| (q - p).perp_dot(r - p);
    let (pa, pb, pc) = (a.truncate(), b.truncate(), c.truncate());
    let area = edge(pa, pb, pc);
    if area == 0.0 {
        return;
    }
    let low = (pa.min(pb).min(pc) - 0.5).ceil().max(Vec2::ZERO);
    let high = (pa.max(pb).max(pc) - 0.5).floor().min(Vec2::splat(width as f32 - 1.0));
    if low.x > high.x || low.y > high.y {
        return;
    }
    for y in low.y as usize..=high.y as usize {
        for x in low.x as usize..=high.x as usize {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let (u, v) = (edge(pb, pc, p) / area, edge(pc, pa, p) / area);
            let w = 1.0 - u - v;
            if u >= -1e-5 && v >= -1e-5 && w >= -1e-5 {
                put(y * width + x, u * a.z + v * b.z + w * c.z);
            }
        }
    }
}
