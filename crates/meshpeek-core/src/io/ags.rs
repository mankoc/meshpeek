//! RuneBrace `.ags` support projects: one model with its supports, as a JSON file. Worked out from sample files and
//! a RuneBrace screenshot; `docs/formats/AGS_NOTES.md` describes it.
//!
//! In short:
//!
//! - `ModifiedMeshDataBase64Gz`: the model, gzipped and in Base64: the vertex count (i32), the vertices (3 × f32), the
//!   index count, the triangle indices (i32), the normal count and the normals. Without it, `ModelPath` names the
//!   model file, which is only opened when the caller allows other files;
//! - `ModelTransform`: the model's matrix (`M11`…`M44`, row vectors), into a world with Y up and the plate at Y = 0;
//! - `Supports`: in that world, each with `ControlPoints` from the tip to the foot and a radius for each point.
//!
//! The supports carry no triangles, so their shapes are built here. The raft and the bracing are not stored either:
//! they are rebuilt from the feet and from the `Bracing_*` settings.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use base64::Engine;
use glam::{DMat4, DVec2, DVec3};
use rayon::prelude::*;
use serde_json::Value;

use crate::error::{LoadError, Result};
use crate::io::lys::raft::loft;
use crate::io::lys::supports::{capsule, frustum, sphere};
use crate::io::{Y_UP_TO_Z_UP, load_mesh, stem};
use crate::mesh::Mesh;
use crate::scene::{ObjectKind, Scene, SceneObject};

/// The lowest raft thickness taken as "on the plate" when looking for feet (they end half a raft above the plate).
const MIN_FOOT_HEIGHT: f64 = 0.3;
/// RuneBrace's "Network Parameters" of a `Networked` raft, which the projects do not store (these are the values of
/// the project they were read from): how far a pad reaches beyond its foot, the width of the bridges between pads
/// and their longest length.
const ISLAND_MARGIN: f64 = 1.0;
const BRIDGE_WIDTH: f64 = 0.5;
const MAX_BRIDGE_LENGTH: f64 = 10.0;
/// RuneBrace's "Max Conn": the rods one rod is braced to, which the projects do not store either.
const MAX_BRACED_RODS: usize = 5;
/// Side of the cells of an [`Underside`], in millimetres.
const UNDERSIDE_CELL: f64 = 1.0;

/// One support, in this viewer's Z-up world.
struct Support {
    points: Vec<DVec3>,
    radii: Vec<f64>,
    /// A pillar inside a hollow model: it has neither a tip nor a foot.
    column: bool,
    tip_sphere: bool,
}

/// What the project says about all supports.
struct Style {
    tip_sphere: f64,
    /// Radius of the foot cones compared with the support (1 without reinforcement).
    foot: f64,
    /// Height of the foot cones.
    foot_cone: f64,
    raft_thickness: f64,
}

/// Parse an `.ags` file; `path` names the file in messages and labels the objects. With `external_files`, a project
/// without a stored model loads the file `ModelPath` names, next to `path`; otherwise it is refused.
pub fn load_bytes(data: &[u8], path: &Path, support_sides: u32, external_files: bool) -> Result<Scene> {
    let bad = |what: String| LoadError::format(format!("{}: {what}", path.display()));
    let project: Value = serde_json::from_slice(data).map_err(|e| bad(format!("not a JSON project ({e})")))?;
    let supports_field = project.get("Supports").and_then(Value::as_array);
    let model_path = project.get("ModelPath").and_then(Value::as_str).unwrap_or_default();
    let packed = project.get("ModifiedMeshDataBase64Gz").and_then(Value::as_str).unwrap_or_default();
    if supports_field.is_none() && packed.is_empty() {
        return Err(bad("not a support project (no model and no supports)".into()));
    }
    let mesh = if packed.is_empty() {
        if !external_files {
            return Err(bad("the model is not stored in the project".into()));
        }
        let file = path.parent().unwrap_or(Path::new("")).join(model_path);
        load_mesh(&file).map_err(|e| bad(format!("the model is not stored in the project: {e}")))?
    } else {
        unpack_mesh(packed).map_err(bad)?
    };

    let number = |key: &str, default: f64| project.get(key).and_then(Value::as_f64).unwrap_or(default);
    let flag = |key: &str, default: bool| project.get(key).and_then(Value::as_bool).unwrap_or(default);
    let style = Style {
        tip_sphere: if flag("UseTipSphere", true) { number("TipSphereMultiplier", 1.0) } else { 0.0 },
        foot: if flag("ReinforcementEnabled", false) {
            number("ReinforcementBaseRadiusPercent", 100.0) / 100.0
        } else {
            1.0
        },
        foot_cone: number("ReinforcementConeHeightMM", 1.0),
        raft_thickness: number("RaftThickness", 0.0),
    };
    let supports: Vec<Support> = supports_field.into_iter().flatten().filter_map(read_support).collect();
    let sides = support_sides.max(3) as usize;
    let support_mesh = build_supports(&supports, &style, sides);
    let transform = Y_UP_TO_Z_UP * matrix(project.get("ModelTransform"));
    let bracing = Bracing {
        min_length: number("Bracing_MinBracingLength", 1.0),
        max_length: number("Bracing_MaxBracingLength", 8.8),
        angle: number("Bracing_OptimalAngleDegrees", 45.0).clamp(5.0, 85.0).to_radians(),
        radius: number("Bracing_BracingRadiusFactor", 0.5),
        heights: (number("Bracing_MinHeight", 0.02), number("Bracing_MaxHeight", 0.9)),
        separation: number("Bracing_VerticalSeparationMM", 2.0).max(0.0),
        crossed: flag("Bracing_UseCrossedBracing", true),
        clearance: number("Bracing_ClearanceForCollisionCheck", 0.8),
    };
    let underside = Underside::of(&mesh, transform);
    let braces = braces(&supports, &style, &bracing, &underside);

    let name = if model_path.is_empty() { stem(path) } else { stem(Path::new(model_path)) };
    let mut model = SceneObject::new(name, mesh, path);
    model.transform = transform;
    let mut info = vec![("Supports".to_string(), supports.len().to_string())];
    if flag("IsHollowed", false) {
        let wall = number("HollowThickness", 0.0);
        info.push(("Hollowed".to_string(), format!("{wall:.2} mm wall (not shown)")));
    }
    if let Some(holes) = project.get("Holes").and_then(Value::as_array).filter(|h| !h.is_empty()) {
        info.push(("Holes".to_string(), holes.len().to_string()));
    }
    if !model_path.is_empty() {
        info.push(("Model file".to_string(), model_path.to_string()));
    }
    model.info = info;

    let mut scene = Scene::default();
    scene.objects.push(model);
    if !support_mesh.faces.is_empty() {
        let mut child = SceneObject::new("Supports", support_mesh, path);
        child.kind = ObjectKind::Support;
        child.parent = Some(0);
        scene.objects.push(child);
    }
    if !braces.is_empty() {
        let pieces = braces.par_iter().map(|b| capsule(b.0, b.1, b.2, b.2, true, true, sides, 1, 1));
        let mut child = SceneObject::new("Bracing", Mesh::merge(pieces.collect::<Vec<_>>()), path);
        child.kind = ObjectKind::Support;
        child.parent = Some(0);
        scene.objects.push(child);
    }
    Ok(scene)
}

/// The model stored in the project.
fn unpack_mesh(packed: &str) -> std::result::Result<Mesh, String> {
    let gzipped = base64::engine::general_purpose::STANDARD
        .decode(packed)
        .map_err(|e| format!("the stored model is not valid Base64 ({e})"))?;
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(&gzipped[..])
        .read_to_end(&mut raw)
        .map_err(|e| format!("the stored model cannot be unpacked ({e})"))?;
    parse_mesh(&raw)
}

/// Vertices and triangles of an unpacked model (the normals after them are worked out again by the renderer).
fn parse_mesh(raw: &[u8]) -> std::result::Result<Mesh, String> {
    let short = || "the stored model is cut short".to_string();
    let count = |at: usize| -> std::result::Result<usize, String> {
        let bytes = raw.get(at..at.checked_add(4).ok_or_else(short)?).ok_or_else(short)?;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()) as usize)
    };
    let triple = |t: &[u8]| [0, 1, 2].map(|k| u32::from_le_bytes(t[k * 4..k * 4 + 4].try_into().unwrap()));
    let vertex_count = count(0)?;
    let indices_at = vertex_count.checked_mul(12).and_then(|n| n.checked_add(8)).ok_or_else(short)?;
    let index_count = count(indices_at - 4)?;
    let end = index_count.checked_mul(4).and_then(|n| n.checked_add(indices_at)).ok_or_else(short)?;
    if end > raw.len() || index_count % 3 != 0 {
        return Err(short());
    }
    let vertices: Vec<[f32; 3]> =
        raw[4..indices_at - 4].par_chunks_exact(12).map(|v| triple(v).map(f32::from_bits)).collect();
    let faces: Vec<[u32; 3]> = raw[indices_at..end].par_chunks_exact(12).map(triple).collect();
    if faces.par_iter().any(|f| f.iter().any(|&i| i as usize >= vertices.len())) {
        return Err("the stored model refers to a vertex that does not exist".into());
    }
    if faces.is_empty() {
        return Err("the stored model has no triangles".into());
    }
    Ok(Mesh::new(vertices, faces))
}

/// `ModelTransform`. Its rows are for row vectors, so they are the columns of this viewer's matrices.
fn matrix(value: Option<&Value>) -> DMat4 {
    let Some(value) = value else { return DMat4::IDENTITY };
    let mut cells = [0.0; 16];
    for (i, cell) in cells.iter_mut().enumerate() {
        let key = format!("M{}{}", i / 4 + 1, i % 4 + 1);
        match value.get(&key).and_then(Value::as_f64) {
            Some(number) if number.is_finite() => *cell = number,
            _ => return DMat4::IDENTITY,
        }
    }
    DMat4::from_cols_array(&cells)
}

/// A support record, its points turned to Z-up (`None` for one that cannot be drawn).
fn read_support(record: &Value) -> Option<Support> {
    let points: Vec<DVec3> = record.get("ControlPoints")?.as_array()?.iter().map(point).collect::<Option<_>>()?;
    let radii: Vec<f64> = record.get("Radii")?.as_array()?.iter().map(Value::as_f64).collect::<Option<_>>()?;
    let flag = |key: &str| record.get(key).and_then(Value::as_bool).unwrap_or(false);
    let usable = points.len() >= 2 && !radii.is_empty() && radii.iter().all(|r| r.is_finite());
    // "UseTIpSphere" is how the files spell it
    usable.then(|| Support { points, radii, column: flag("IsColumn"), tip_sphere: flag("UseTIpSphere") })
}

/// An `{X, Y, Z}` of the Y-up world, in this viewer's Z-up world.
fn point(value: &Value) -> Option<DVec3> {
    let axis = |key: &str| value.get(key)?.as_f64().filter(|v| v.is_finite());
    Some(DVec3::new(axis("X")?, -axis("Z")?, axis("Y")?))
}

impl Support {
    /// The radius at point `index` (a support can list fewer radii than points).
    fn radius(&self, index: usize) -> f64 {
        self.radii[index.min(self.radii.len() - 1)].abs().max(1e-3)
    }

    /// Where it stands on the plate, if it does.
    fn foot(&self, style: &Style) -> Option<DVec3> {
        let last = *self.points.last()?;
        (!self.column && last.z <= style.raft_thickness.max(MIN_FOOT_HEIGHT)).then_some(last)
    }

    /// The part of a support standing on the plate that can be braced: from the foot up, without the tip.
    fn rod(&self, style: &Style) -> Option<Rod> {
        self.foot(style)?;
        let from = usize::from(self.points.len() > 2);
        let points = self.points[from..].iter().rev().copied().collect();
        Some(Rod { points, radius: self.radius(usize::MAX) })
    }

    /// Its shape: a capsule for each stretch between two points, a sphere on the tip, a cone on the plate.
    fn pieces(&self, style: &Style, sides: usize) -> Vec<Mesh> {
        let rings = (sides / 2).max(2);
        let last = self.points.len() - 1;
        let on_plate = self.foot(style).is_some();
        let mut pieces = Vec::with_capacity(last + 2);
        for (i, ends) in self.points.windows(2).enumerate() {
            let (top, bottom, radius) = (ends[0], ends[1], self.radius(i + 1));
            if !(on_plate && i + 1 == last) {
                pieces.push(capsule(top, bottom, self.radius(i), radius, false, false, sides, rings, rings));
                continue;
            }
            // the last stretch keeps its radius down to the cone
            let height = style.foot_cone.clamp(0.0, top.distance(bottom));
            let cone_top = bottom + (top - bottom).normalize_or_zero() * height;
            pieces.push(capsule(top, cone_top, self.radius(i), radius, false, false, sides, rings, rings));
            pieces.push(capsule(cone_top, bottom, radius, radius * style.foot, false, true, sides, rings, rings));
        }
        if self.tip_sphere && !self.column && style.tip_sphere > 0.0 {
            pieces.push(sphere(self.points[0], self.radius(0) * style.tip_sphere, sides));
        }
        pieces
    }
}

/// All supports and the raft under them as one mesh.
fn build_supports(supports: &[Support], style: &Style, sides: usize) -> Mesh {
    let mut pieces: Vec<Mesh> = supports.par_iter().flat_map_iter(|s| s.pieces(style, sides)).collect();
    pieces.extend(raft(supports, style, sides));
    Mesh::merge(pieces)
}

/// The raft: a round pad under each foot, and bridges between them. The projects do not store it; this follows what
/// RuneBrace shows for a `Networked` raft, though not necessarily bridge for bridge.
fn raft(supports: &[Support], style: &Style, sides: usize) -> Vec<Mesh> {
    let thickness = style.raft_thickness;
    if thickness <= 0.0 {
        return Vec::new();
    }
    // feet by hundredths of a millimetre: several supports can share one
    let feet: BTreeSet<(i64, i64)> = supports
        .iter()
        .filter_map(|s| s.foot(style))
        .map(|f| ((f.x * 100.0).round() as i64, (f.y * 100.0).round() as i64))
        .collect();
    let feet: Vec<DVec2> = feet.iter().map(|f| DVec2::new(f.0 as f64, f.1 as f64) / 100.0).collect();
    let foot = supports.iter().find(|s| s.foot(style).is_some()).map_or(0.0, |s| s.radius(usize::MAX) * style.foot);
    let radius = foot + ISLAND_MARGIN;
    let mut pieces: Vec<Mesh> =
        feet.iter().map(|f| frustum(f.extend(0.0), radius, radius, thickness, sides * 2, 0.0)).collect();
    pieces.extend(bridges(&feet).into_iter().map(|(a, b)| bar(feet[a], feet[b], thickness)));
    pieces
}

/// Which `feet` (sorted) the raft joins: the edges of their Delaunay triangulation up to [`MAX_BRIDGE_LENGTH`].
fn bridges(feet: &[DVec2]) -> Vec<(usize, usize)> {
    let points: Vec<delaunator::Point> = feet.iter().map(|f| delaunator::Point { x: f.x, y: f.y }).collect();
    let triangulation = delaunator::triangulate(&points);
    let edges: Vec<(usize, usize)> = if triangulation.triangles.is_empty() {
        (1..feet.len()).map(|i| (i - 1, i)).collect() // all in one line
    } else {
        (0..triangulation.triangles.len())
            .filter(|&e| e < triangulation.halfedges[e]) // each edge once (on the hull the opposite is EMPTY, the largest)
            .map(|e| (triangulation.triangles[e], triangulation.triangles[delaunator::next_halfedge(e)]))
            .collect()
    };
    edges.into_iter().filter(|&(a, b)| feet[a].distance(feet[b]) <= MAX_BRIDGE_LENGTH).collect()
}

/// The `Bracing_*` settings of a project.
struct Bracing {
    min_length: f64,
    max_length: f64,
    /// Rise of a brace above the horizontal, in radians.
    angle: f64,
    /// Radius of a brace compared with its rod.
    radius: f64,
    /// Lowest and highest point of the bracing, as parts of the height of the shorter rod.
    heights: (f64, f64),
    /// Gap between one brace or X and the next one up.
    separation: f64,
    crossed: bool,
    /// Distance kept from the model.
    clearance: f64,
}

/// The lowest point of the model over each cell of the plate.
struct Underside {
    origin: DVec2,
    width: usize,
    lowest: Vec<f32>,
}

impl Underside {
    /// The underside of `mesh`, placed by `transform`.
    fn of(mesh: &Mesh, transform: DMat4) -> Self {
        let placed: Vec<DVec3> =
            mesh.vertices.par_iter().map(|v| transform.transform_point3(DVec3::from_array(v.map(f64::from)))).collect();
        let (lo, hi) = placed
            .iter()
            .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), v| (lo.min(v.truncate()), hi.max(v.truncate())));
        if !lo.is_finite() || !hi.is_finite() {
            return Self { origin: DVec2::ZERO, width: 0, lowest: Vec::new() };
        }
        let width = ((hi.x - lo.x) / UNDERSIDE_CELL) as usize + 1;
        let height = ((hi.y - lo.y) / UNDERSIDE_CELL) as usize + 1;
        let mut lowest = vec![f32::INFINITY; width * height];
        let cell = |p: DVec2| ((p - lo) / UNDERSIDE_CELL).as_uvec2();
        for face in &mesh.faces {
            let [a, b, c] = face.map(|i| placed[i as usize]);
            let (min, max) = (a.min(b).min(c), a.max(b).max(c));
            let (from, to) = (cell(min.truncate()), cell(max.truncate()));
            for y in from.y..=to.y {
                let row = &mut lowest[y as usize * width..][..width];
                for z in &mut row[from.x as usize..=to.x as usize] {
                    *z = z.min(min.z as f32);
                }
            }
        }
        Self { origin: lo, width, lowest }
    }

    /// The model's lowest point over `at` (infinite where there is no model).
    fn height(&self, at: DVec2) -> f64 {
        let cell = (at - self.origin) / UNDERSIDE_CELL;
        if cell.x < 0.0 || cell.y < 0.0 || cell.x as usize >= self.width {
            return f64::INFINITY;
        }
        self.lowest.get(cell.y as usize * self.width + cell.x as usize).map_or(f64::INFINITY, |z| *z as f64)
    }

    /// The straight line from `a` to `b` stays `clearance` below the model.
    fn clear(&self, a: DVec3, b: DVec3, clearance: f64) -> bool {
        let steps = (a.distance(b) / (UNDERSIDE_CELL / 2.0)).ceil().max(1.0) as usize;
        (0..=steps).all(|i| {
            let p = a.lerp(b, i as f64 / steps as f64);
            p.z + clearance < self.height(p.truncate())
        })
    }
}

/// A rod to brace, its points from the foot up.
struct Rod {
    points: Vec<DVec3>,
    radius: f64,
}

impl Rod {
    fn top(&self) -> f64 {
        self.points.last().map_or(0.0, |p| p.z)
    }

    /// The point of the rod at height `z`: rods lean on their way up.
    fn at(&self, z: f64) -> DVec3 {
        let stretch = self.points.windows(2).find(|s| z <= s[1].z).unwrap_or(&self.points[self.points.len() - 2..]);
        let rise = stretch[1].z - stretch[0].z;
        let t = if rise.abs() > 1e-9 { ((z - stretch[0].z) / rise).clamp(0.0, 1.0) } else { 0.0 };
        stretch[0].lerp(stretch[1], t).truncate().extend(z)
    }
}

/// The bracing, as (start, end, radius): RuneBrace works it out from its settings, so these are not its exact braces.
/// Rods close enough for a brace at the wanted angle are joined, nearest first, by braces (or Xs) stacked from the
/// bottom up; those that would come too close to the model are left out.
fn braces(supports: &[Support], style: &Style, bracing: &Bracing, underside: &Underside) -> Vec<(DVec3, DVec3, f64)> {
    let rods: Vec<Rod> = supports.iter().filter_map(|s| s.rod(style)).filter(|r| r.points.len() >= 2).collect();
    let lengths = bracing.min_length..=bracing.max_length;
    let mut pairs: Vec<(f64, usize, usize)> = (0..rods.len())
        .flat_map(|a| (a + 1..rods.len()).map(move |b| (a, b)))
        .map(|(a, b)| (rods[a].points[0].truncate().distance(rods[b].points[0].truncate()), a, b))
        .filter(|&(distance, ..)| distance > 1e-3 && lengths.contains(&(distance / bracing.angle.cos())))
        .collect();
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut braced = vec![0; rods.len()];
    let mut out = Vec::new();
    for (_, a, b) in pairs {
        if braced[a] == MAX_BRACED_RODS || braced[b] == MAX_BRACED_RODS {
            continue;
        }
        let (rod_a, rod_b) = (&rods[a], &rods[b]);
        let top = rod_a.top().min(rod_b.top());
        let (mut low, high) = (top * bracing.heights.0, top * bracing.heights.1);
        let mut joined = false;
        while low < high {
            let distance = rod_a.at(low).truncate().distance(rod_b.at(low).truncate());
            let rise = distance * bracing.angle.tan();
            if low + rise > high {
                break;
            }
            if lengths.contains(&(distance / bracing.angle.cos())) {
                let up = (rod_a.at(low), rod_b.at(low + rise));
                let down = (rod_b.at(low), rod_a.at(low + rise));
                for (from, to) in std::iter::once(up).chain(bracing.crossed.then_some(down)) {
                    if underside.clear(from, to, bracing.clearance) {
                        out.push((from, to, rod_a.radius * bracing.radius));
                        joined = true;
                    }
                }
            }
            low += rise + bracing.separation.max(0.1);
        }
        if joined {
            braced[a] += 1;
            braced[b] += 1;
        }
    }
    out
}

/// A flat bridge of the raft from `a` to `b`, standing on the plate.
fn bar(a: DVec2, b: DVec2, thickness: f64) -> Mesh {
    let side = (b - a).normalize_or_zero().perp() * (BRIDGE_WIDTH / 2.0);
    let corners = [a - side, b - side, b + side, a + side];
    loft(&corners, 0.0, &corners, thickness)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    /// A one-triangle model as a project stores it.
    fn packed_triangle() -> String {
        let mut raw: Vec<u8> = 3u32.to_le_bytes().to_vec();
        [0f32, 0., 0., 2., 0., 0., 0., 2., 0.].iter().for_each(|v| raw.extend(v.to_le_bytes()));
        raw.extend(3u32.to_le_bytes());
        [0u32, 1, 2].iter().for_each(|i| raw.extend(i.to_le_bytes()));
        raw.extend(3u32.to_le_bytes());
        raw.extend([0u8; 36]); // the normals
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(&raw).unwrap();
        base64::engine::general_purpose::STANDARD.encode(gzip.finish().unwrap())
    }

    fn project(extra: &str) -> Vec<u8> {
        let transform: Vec<String> = (0..16)
            .map(|i| {
                let value = match i {
                    0 | 5 | 10 | 15 => 1.0,
                    13 => 5.0, // M42: 5 up in the Y-up world
                    _ => 0.0,
                };
                format!("\"M{}{}\": {value}", i / 4 + 1, i % 4 + 1)
            })
            .collect();
        format!(
            r#"{{"ModelPath": "Part.stl", "ModifiedMeshDataBase64Gz": "{}", "ModelTransform": {{{}}}, {extra}
               "RaftThickness": 0.5, "GridSize": 4, "ReinforcementEnabled": true,
               "ReinforcementBaseRadiusPercent": 150, "TipSphereMultiplier": 1.13, "UseTipSphere": true}}"#,
            packed_triangle(),
            transform.join(", ")
        )
        .into_bytes()
    }

    const SUPPORTS: &str = r#""Supports": [
        {"ControlPoints": [{"X": 0, "Y": 5, "Z": 0}, {"X": 0, "Y": 1.25, "Z": 0}, {"X": 0, "Y": 0.25, "Z": 0}],
         "Radii": [0.2, 0.5, 0.5], "UseTIpSphere": true, "IsColumn": false},
        {"ControlPoints": [{"X": 4, "Y": 5, "Z": 0}, {"X": 4, "Y": 0.25, "Z": 0}],
         "Radii": [0.2, 0.5], "UseTIpSphere": true, "IsColumn": false},
        {"ControlPoints": [{"X": 1, "Y": 9, "Z": 1}, {"X": 1, "Y": 7, "Z": 1}],
         "Radii": [0.5, 0.5], "UseTIpSphere": true, "IsColumn": true}],"#;

    #[test]
    fn model_is_placed_and_supports_stand_on_the_plate_with_z_up() {
        let scene = load_bytes(&project(SUPPORTS), Path::new("p.ags"), 8, false).unwrap();
        assert_eq!(scene.objects.len(), 2);
        let model = &scene.objects[0];
        assert_eq!((model.name.as_str(), model.mesh.n_faces()), ("Part", 1));
        // a vertex at Y = 2 in the model is 7 above the plate: Y up becomes Z up
        let top = model.transform.transform_point3(DVec3::new(0.0, 2.0, 0.0));
        assert!(top.distance(DVec3::new(0.0, 0.0, 7.0)) < 1e-9);
        let supports = &scene.objects[1];
        assert_eq!((supports.kind, supports.parent), (ObjectKind::Support, Some(0)));
        let (lo, hi) = supports.mesh.bounds().unwrap();
        assert!(lo[2].abs() < 1e-4, "the raft lies on the plate, got {}", lo[2]);
        assert!(hi[2] > 9.0, "the column is there, got {}", hi[2]);
    }

    #[test]
    fn feet_get_pads_joined_by_bars() {
        let at = |x: f64, y: f64| Support {
            points: vec![DVec3::new(x, y, 3.0), DVec3::new(x, y, 0.25)],
            radii: vec![0.5],
            column: false,
            tip_sphere: false,
        };
        let style = Style { tip_sphere: 0.0, foot: 1.5, foot_cone: 1.0, raft_thickness: 0.5 };
        let square = [at(0.0, 0.0), at(0.0, 4.0), at(4.0, 0.0), at(4.0, 4.0)];
        assert_eq!(raft(&square, &style, 8).len(), 4 + 5); // the sides and one diagonal
        assert_eq!(raft(&[at(0.0, 0.0), at(4.0, 0.0), at(8.0, 0.0)], &style, 8).len(), 3 + 2); // in a line
        // the cone is as high as the project says, above a support of even width
        assert_eq!(square[0].pieces(&style, 8).len(), 2);
    }

    #[test]
    fn bridges_stop_at_the_longest_length() {
        let feet = [(0., 0.), (0., 4.), (4., 0.), (4., 4.), (60., 2.)].map(|(x, y)| DVec2::new(x, y));
        let links = bridges(&feet);
        assert_eq!(links.len(), 5, "{links:?}"); // the far foot stays on its own
        assert!(links.iter().all(|(a, b)| *a != 4 && *b != 4));
    }

    #[test]
    fn bars_face_outwards() {
        let mesh = bar(DVec2::ZERO, DVec2::new(4.0, 0.0), 0.5);
        let corner = |i: u32| DVec3::from_array(mesh.vertices[i as usize].map(f64::from));
        let volume: f64 = mesh.faces.iter().map(|f| corner(f[0]).dot(corner(f[1]).cross(corner(f[2]))) / 6.0).sum();
        assert!((volume - 4.0 * BRIDGE_WIDTH * 0.5).abs() < 1e-5, "{volume}");
    }

    #[test]
    fn neighbouring_columns_get_stacked_xs_that_stay_clear_of_the_model() {
        let column = |x: f64, y: f64| Support {
            points: vec![DVec3::new(x, y, 30.0), DVec3::new(x, y, 0.25)],
            radii: vec![0.5],
            column: false,
            tip_sphere: false,
        };
        let style = Style { tip_sphere: 0.0, foot: 1.5, foot_cone: 1.0, raft_thickness: 0.5 };
        let bracing = Bracing {
            min_length: 1.0,
            max_length: 8.8,
            angle: 45f64.to_radians(),
            radius: 0.5,
            heights: (0.02, 0.9),
            separation: 2.0,
            crossed: true,
            clearance: 0.8,
        };
        let open = Underside { origin: DVec2::ZERO, width: 0, lowest: Vec::new() };
        let pair = [column(0.0, 0.0), column(4.0, 0.0)];
        let found = braces(&pair, &style, &bracing, &open);
        assert_eq!(found.len(), 4 * 2); // from 0.6 to 27, 4 up and a gap of 2: four Xs
        assert_eq!(found[0].2, 0.25);
        assert!((found[0].1.z - found[0].0.z - 4.0).abs() < 1e-9); // 45 degrees
        // too far apart for the longest brace at this angle
        assert!(braces(&[column(0.0, 0.0), column(8.0, 0.0)], &style, &bracing, &open).is_empty());
        // a model at height 10 over all of it leaves the first X only
        let low = Underside { origin: DVec2::splat(-1.0), width: 8, lowest: vec![10.0; 8 * 8] };
        assert_eq!(braces(&pair, &style, &bracing, &low).len(), 2);
    }

    #[test]
    fn rods_lean_and_their_tips_are_not_braced() {
        let support = Support {
            points: vec![
                DVec3::new(9.0, 0.0, 22.0),
                DVec3::new(2.0, 0.0, 20.0),
                DVec3::new(0.0, 0.0, 10.0),
                DVec3::new(0.0, 0.0, 0.25),
            ],
            radii: vec![0.2, 0.5],
            column: false,
            tip_sphere: false,
        };
        let style = Style { tip_sphere: 0.0, foot: 1.5, foot_cone: 1.0, raft_thickness: 0.5 };
        let rod = support.rod(&style).unwrap();
        assert_eq!(rod.top(), 20.0);
        assert_eq!(rod.at(5.0), DVec3::new(0.0, 0.0, 5.0));
        assert_eq!(rod.at(15.0), DVec3::new(1.0, 0.0, 15.0));
    }

    #[test]
    fn underside_is_the_lowest_point_over_each_cell() {
        let mesh = Mesh::new(vec![[0., 0., 5.], [3., 0., 5.], [0., 3., 5.]], vec![[0, 1, 2]]);
        let underside = Underside::of(&mesh, DMat4::IDENTITY);
        assert_eq!(underside.height(DVec2::new(1.0, 1.0)), 5.0);
        assert_eq!(underside.height(DVec2::new(-4.0, 1.0)), f64::INFINITY);
        assert!(underside.clear(DVec3::new(1.0, 1.0, 1.0), DVec3::new(2.0, 1.0, 4.0), 0.8));
        assert!(!underside.clear(DVec3::new(1.0, 1.0, 1.0), DVec3::new(2.0, 1.0, 4.5), 0.8));
    }

    #[test]
    fn broken_projects_are_reported() {
        assert!(
            load_bytes(b"{}", Path::new("p.ags"), 8, false).unwrap_err().to_string().contains("not a support project")
        );
        assert!(load_bytes(b"not json", Path::new("p.ags"), 8, false).is_err());
        let external = br#"{"Supports": [], "ModelPath": "../secret.stl"}"#;
        let refused = load_bytes(external, Path::new("p.ags"), 8, false).unwrap_err().to_string();
        assert!(refused.contains("not stored in the project"), "{refused}");
        assert!(parse_mesh(&[255, 255, 255, 255]).unwrap_err().contains("cut short"));
    }
}
