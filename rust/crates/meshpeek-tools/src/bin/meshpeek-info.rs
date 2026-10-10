//! Load a file and print its objects, timings and memory use.
//!
//!     meshpeek-info <file> [--support-sides N] [--z-up]

use std::path::PathBuf;
use std::time::Instant;

use meshpeek_core::{LoadOptions, ObjectKind, load};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut file = None;
    let mut options = LoadOptions::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--support-sides" => options.support_sides = args.next().and_then(|n| n.parse().ok()).expect("number"),
            "--z-up" => options.obj_y_up = false,
            _ => file = Some(PathBuf::from(arg)),
        }
    }
    let Some(file) = file else {
        eprintln!("usage: meshpeek-info <file> [--support-sides N] [--z-up]");
        std::process::exit(2);
    };

    let before = memory_mb();
    let start = Instant::now();
    let scene = match load(&file, &options) {
        Ok(scene) => scene,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let seconds = start.elapsed().as_secs_f64();

    let mut triangles = 0;
    for obj in &scene.objects {
        let indent = if obj.parent.is_some() { "  " } else { "" };
        let kind = if obj.kind == ObjectKind::Support { "support" } else { "model" };
        println!(
            "{indent}{:<20} {kind:<8} {:>10} vertices {:>10} triangles{}",
            obj.name,
            obj.mesh.vertices.len(),
            obj.mesh.n_faces(),
            if obj.mesh.colors.is_some() { "  (vertex colours)" } else { "" }
        );
        triangles += obj.mesh.n_faces();
    }
    for reason in &scene.skipped {
        println!("skipped: {reason}");
    }
    for plate in &scene.plates {
        println!("plate: {:?} mm at {:?}", plate.size, plate.position);
    }
    println!(
        "\n{} objects, {triangles} triangles, loaded in {:.0} ms, memory +{:.0} MB (process {:.0} MB)",
        scene.objects.len(),
        seconds * 1000.0,
        memory_mb() - before,
        memory_mb()
    );
}

fn memory_mb() -> f64 {
    memory_stats::memory_stats().map_or(0.0, |m| m.physical_mem as f64 / (1024.0 * 1024.0))
}
