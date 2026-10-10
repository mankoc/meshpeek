//! Compare the Rust loaders with the Python reference dumps (`tools/dump_reference.py` ->
//! `samples/reference/*.json`). Counts must match exactly; coordinates within a small tolerance.
//!
//! Skips (with a message) when the sample files are missing or are un-fetched Git LFS pointers.

use std::path::{Path, PathBuf};

use glam::DVec3;
use meshpeek_core::{LoadOptions, ObjectKind, Scene, load};
use serde_json::Value;

fn samples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples")
}

fn available(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|b| !b.starts_with(b"version https://git-lfs"))
}

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance * (1.0 + b.abs())
}

fn check_vec(label: &str, got: [f64; 3], want: &Value, tolerance: f64) {
    for i in 0..3 {
        let w = want[i].as_f64().unwrap();
        assert!(close(got[i], w, tolerance), "{label}[{i}]: got {}, want {w}", got[i]);
    }
}

fn check_scene(name: &str, scene: &Scene, reference: &Value) {
    let objects = reference["objects"].as_array().unwrap();
    assert_eq!(scene.objects.len(), objects.len(), "{name}: object count");
    assert_eq!(scene.skipped.len(), reference["skipped"].as_array().unwrap().len(), "{name}: skipped");
    assert_eq!(scene.plates.len(), reference["plates"].as_array().unwrap().len(), "{name}: plates");
    for (plate, want) in scene.plates.iter().zip(reference["plates"].as_array().unwrap()) {
        check_vec(&format!("{name} plate size"), plate.size, &want["size"], 1e-9);
    }
    for (obj, want) in scene.objects.iter().zip(objects) {
        let label = format!("{name}/{}", obj.name);
        assert_eq!(obj.name, want["name"].as_str().unwrap(), "{label}: name");
        let kind = if obj.kind == ObjectKind::Support { "support" } else { "model" };
        assert_eq!(kind, want["kind"].as_str().unwrap(), "{label}: kind");
        let parent = obj.parent.map(|p| scene.objects[p].name.as_str());
        assert_eq!(parent, want["parent"].as_str(), "{label}: parent");
        assert_eq!(obj.mesh.vertices.len() as u64, want["n_vertices"].as_u64().unwrap(), "{label}: vertices");
        assert_eq!(obj.mesh.n_faces() as u64, want["n_faces"].as_u64().unwrap(), "{label}: faces");
        assert_eq!(obj.mesh.colors.is_some(), want["has_colors"].as_bool().unwrap(), "{label}: colours");

        let (lo, hi) = obj.mesh.bounds().unwrap();
        check_vec(&format!("{label} bounds_min"), lo.map(f64::from), &want["bounds_min"], 1e-6);
        check_vec(&format!("{label} bounds_max"), hi.map(f64::from), &want["bounds_max"], 1e-6);
        let n = obj.mesh.vertices.len() as f64;
        let centroid = obj.mesh.vertices.iter().fold(DVec3::ZERO, |s, v| s + DVec3::from_array(v.map(f64::from))) / n;
        check_vec(&format!("{label} centroid"), centroid.to_array(), &want["centroid"], 1e-6);

        for (r, row) in want["transform"].as_array().unwrap().iter().enumerate() {
            check_vec(&format!("{label} transform row {r}"), obj.transform.row(r).truncate().to_array(), row, 1e-9);
            let t = row[3].as_f64().unwrap();
            assert!(close(obj.transform.row(r).w, t, 1e-9), "{label}: transform[{r}][3]");
        }
        let world: Vec<DVec3> = obj
            .mesh
            .vertices
            .iter()
            .map(|v| obj.transform.transform_point3(DVec3::from_array(v.map(f64::from))))
            .collect();
        let world_min = world.iter().fold(DVec3::INFINITY, |a, &b| a.min(b));
        let world_max = world.iter().fold(DVec3::NEG_INFINITY, |a, &b| a.max(b));
        check_vec(&format!("{label} world_min"), world_min.to_array(), &want["world_min"], 1e-6);
        check_vec(&format!("{label} world_max"), world_max.to_array(), &want["world_max"], 1e-6);
    }
}

#[test]
fn matches_python_reference() {
    let reference_dir = samples().join("reference");
    let Ok(entries) = std::fs::read_dir(&reference_dir) else {
        eprintln!("skipping: {} not found", reference_dir.display());
        return;
    };
    let mut checked = 0;
    for entry in entries {
        let path = entry.unwrap().path();
        let reference: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let file = samples().join(reference["file"].as_str().unwrap());
        if !available(&file) {
            eprintln!("skipping {}: sample not available (run `git lfs pull`)", file.display());
            continue;
        }
        let options = LoadOptions {
            support_sides: reference["options"]["support_sides"].as_u64().unwrap() as u32,
            obj_y_up: reference["options"]["obj_y_up"].as_bool().unwrap(),
        };
        let scene = load(&file, &options).unwrap();
        check_scene(path.file_stem().unwrap().to_str().unwrap(), &scene, &reference);
        checked += 1;
    }
    eprintln!("checked {checked} reference cases");
}
