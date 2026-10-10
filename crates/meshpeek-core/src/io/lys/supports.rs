//! Support geometry from Lychee's parametric support records.
//!
//! A plate support is drawn as: foot -> vertical column up to the knee (`base.length + base.joinLength` above
//! the plate, along `baseNormal`) -> angled column to the end of the tip cone -> tip cone touching the model.
//! A branch support (`isBaseTip`) starts on its parent's column and runs to its own tip cone.
//! A brace (`parentTipId` set) has its tip on another support instead of the model: plain cylinder, no cone.
//! Bends (knee, tip cone end) get a sphere so the joints look rounded instead of cut. Only the part of each
//! sphere outside both adjoining segments is kept; branch starts need none (they sit inside the parent).
//! Segment ends are only capped where they are exposed (the tip touching the model, the underside of the
//! foot); every other end is hidden inside a joint, the next segment or a parent support.
//!
//! Coordinates (checked against sample files): `tip` is in the tip object's space. Branch `base` points are in
//! the base object's space and lie exactly on the parent column. Plate `base` points are at plate height with
//! x/y relative to the base object's position.
//!
//! Not drawn: Lychee's bracing lattice between grid supports. It is generated from bracing presets at runtime
//! and is not stored in the file.

use std::collections::HashMap;
use std::f64::consts::PI;

use glam::{DMat4, DVec3};
use rmpv::Value;

use super::value::{get, ids, index, number, str_of, truthy, vec3};
use crate::mesh::Mesh;

/// Default facets around each support column.
pub const SIDES: u32 = 12;

/// Joint spheres must stay inside the columns: an n-sided prism's flat sides sit at `r * cos(pi / n)`, so a
/// full-radius sphere would poke out between them. 0.99 keeps the surfaces from z-fighting.
pub fn joint_scale(sides: u32) -> f64 {
    (PI / sides as f64).cos() * 0.99
}

#[derive(Default)]
struct Builder {
    segments: Vec<Segment>,
    joints: Vec<Joint>,
}

struct Segment {
    p0: DVec3,
    p1: DVec3,
    r0: f64,
    r1: f64,
    cap0: bool,
    cap1: bool,
}

pub struct Joint {
    pub centre: DVec3,
    pub radius: f64,
    /// Directions in which the two adjoining segments leave the joint.
    pub arm_a: DVec3,
    pub arm_b: DVec3,
}

fn unit(v: DVec3, fallback: DVec3) -> DVec3 {
    let length = v.length();
    if length > 1e-9 { v / length } else { fallback }
}

/// World-space support meshes per model: tip object id -> all visible supports touching that object.
///
/// `transforms` holds the placed objects' matrices; `objects` their Lychee records (position, grid mode).
pub fn build(
    supports: Option<&Value>,
    transforms: &HashMap<&str, DMat4>,
    objects: &HashMap<&str, &Value>,
    sides: u32,
) -> HashMap<String, Mesh> {
    let Some(supports) = supports else { return HashMap::new() };
    let by_id = index(get(supports, "byId"));
    let mut builders: HashMap<String, Builder> = HashMap::new();

    for sid in ids(get(supports, "allIds")) {
        let Some(&s) = by_id.get(sid) else { continue };
        let Some(tip_id) = str_of(s, "objectIdTip") else { continue };
        let Some(tip_matrix) = transforms.get(tip_id) else { continue };
        if get(s, "isVisible").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let b = builders.entry(tip_id.to_string()).or_default();
        let mut segment = |p0, p1, r0, r1, cap0, cap1| b.segments.push(Segment { p0, p1, r0, r1, cap0, cap1 });
        let mut joints = Vec::new();
        let mut joint = |centre: DVec3, radius, toward_a: DVec3, toward_b: DVec3| {
            joints.push(Joint {
                centre,
                radius,
                arm_a: unit(toward_a - centre, DVec3::Z),
                arm_b: unit(toward_b - centre, -DVec3::Z),
            })
        };

        let settings = get(s, "settings");
        let tip_cfg = settings.and_then(|c| get(c, "tip"));
        let base_cfg = settings.and_then(|c| get(c, "base"));
        let cfg = |c: Option<&Value>, key: &str, default: f64| c.map_or(default, |c| number(c, key, default));
        let column_radius = cfg(tip_cfg, "diameter", 1.0) / 2.0;

        let tip = tip_matrix.transform_point3(vec3(get(s, "tip"), 0.0));
        let brace = truthy(s, "parentTipId"); // tip rests on another support (checked: exactly on its column)
        let tip_end = if brace {
            tip
        } else {
            let normal = unit(tip_matrix.transform_vector3(vec3(get(s, "tipNormal"), 0.0)), -DVec3::Z);
            let end = tip + normal * cfg(tip_cfg, "length", 3.0);
            segment(tip, end, cfg(tip_cfg, "pointDiameter", 0.4) / 2.0, column_radius, true, false);
            end
        };

        let base_id = str_of(s, "objectIdBase").filter(|id| transforms.contains_key(id)).unwrap_or(tip_id);
        if truthy(s, "isBaseTip") {
            // Branch: starts on the parent support's column.
            let branch_start = transforms[base_id].transform_point3(vec3(get(s, "base"), 0.0));
            segment(branch_start, tip_end, column_radius, column_radius, false, false);
            if !brace {
                joint(tip_end, column_radius, tip, branch_start);
            }
        } else {
            let base_obj = objects.get(base_id).copied();
            let position = base_obj.map_or(DVec3::ZERO, |o| vec3(get(o, "position"), 0.0));
            let foot = vec3(get(s, "base"), 0.0) + position * DVec3::new(1.0, 1.0, 0.0);
            let up = unit(vec3(get(s, "baseNormal"), 0.0), DVec3::Z);
            let join_radius = cfg(base_cfg, "joinDiameter", 2.0 * column_radius) / 2.0;
            let base_length = cfg(base_cfg, "length", 0.5);
            let (foot_radius, foot_height) = if base_obj.is_some_and(|o| truthy(o, "isSupportGrid")) {
                (2.0 * join_radius, base_length.max(0.5)) // grid supports stand on a lattice: small foot
            } else {
                (cfg(base_cfg, "diameter", 6.0) / 2.0, base_length)
            };
            let foot_top = foot + up * foot_height;
            let mut knee = foot + up * (base_length + cfg(base_cfg, "joinLength", 0.0));
            segment(foot, foot_top, foot_radius, join_radius, true, false);
            if (knee - foot_top).dot(up) > 0.0 {
                segment(foot_top, knee, join_radius, join_radius, false, false);
            } else {
                knee = foot_top;
            }
            segment(knee, tip_end, join_radius, column_radius, false, false);
            if !brace {
                joint(tip_end, column_radius, tip, knee);
            }
            joint(knee, join_radius, foot, tip_end);
        }
        b.joints.extend(joints);
    }

    builders
        .into_iter()
        .map(|(oid, b)| {
            let joints: Vec<Joint> =
                b.joints.into_iter().map(|j| Joint { radius: j.radius * joint_scale(sides), ..j }).collect();
            (oid, Mesh::merge([frustums(&b.segments, sides), joint_spheres(&joints, sides)]))
        })
        .collect()
}

fn to_f32(p: DVec3) -> [f32; 3] {
    [p.x as f32, p.y as f32, p.z as f32]
}

/// Truncated cones between segment end points, capped only where `cap0` / `cap1` say.
fn frustums(segments: &[Segment], sides: u32) -> Mesh {
    let n = sides as usize;
    let angles: Vec<(f64, f64)> = (0..n).map(|k| (2.0 * PI * k as f64 / n as f64).sin_cos()).collect();
    let mut mesh = Mesh::default();
    for s in segments {
        let axis = s.p1 - s.p0;
        let length = axis.length();
        if length <= 1e-6 {
            continue;
        }
        let axis = axis / length;
        let reference = if axis.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
        let u = axis.cross(reference).normalize();
        let v = axis.cross(u);
        let start = mesh.vertices.len() as u32;
        for (p, r) in [(s.p0, s.r0), (s.p1, s.r1)] {
            mesh.vertices.extend(angles.iter().map(|&(sin, cos)| to_f32(p + r * (cos * u + sin * v))));
        }
        mesh.vertices.extend([to_f32(s.p0), to_f32(s.p1)]);
        for a in 0..sides {
            let b = (a + 1) % sides;
            let (a, b, n) = (start + a, start + b, sides);
            mesh.faces.push([a, b, n + b]);
            mesh.faces.push([a, n + b, n + a]);
            if s.cap0 {
                mesh.faces.push([start + 2 * n, b, a]);
            }
            if s.cap1 {
                mesh.faces.push([start + 2 * n + 1, n + a, n + b]);
            }
        }
    }
    mesh
}

/// Low-poly UV sphere around the origin: unit vertices and triangle template.
fn unit_sphere(sides: u32, rings: u32) -> (Vec<DVec3>, Vec<[u32; 3]>) {
    let mut vertices = vec![DVec3::Z];
    for k in 1..rings {
        let (sin_t, cos_t) = (PI * k as f64 / rings as f64).sin_cos();
        vertices.extend((0..sides).map(|j| {
            let (sin_p, cos_p) = (2.0 * PI * j as f64 / sides as f64).sin_cos();
            DVec3::new(sin_t * cos_p, sin_t * sin_p, cos_t)
        }));
    }
    vertices.push(-DVec3::Z);
    let bottom = vertices.len() as u32 - 1;
    let last_ring = 1 + (rings - 2) * sides;
    let mut faces = Vec::new();
    for a in 0..sides {
        let b = (a + 1) % sides;
        faces.push([0, 1 + b, 1 + a]);
        for k in 0..rings - 2 {
            let (top, low) = (1 + k * sides, 1 + (k + 1) * sides);
            faces.push([top + a, top + b, low + b]);
            faces.push([top + a, low + b, low + a]);
        }
        faces.push([bottom, last_ring + a, last_ring + b]);
    }
    (vertices, faces)
}

/// Spheres rounding the bends, minus the triangles hidden inside the two adjoining segments.
///
/// The sphere is narrower than the columns, so everything on a segment's side of the joint (positive dot
/// product with that segment's direction) is inside that segment. What remains is the wedge on the outside
/// of the bend.
pub fn joint_spheres(joints: &[Joint], sides: u32) -> Mesh {
    let (unit, template) = unit_sphere(sides, (sides / 2).max(2));
    let mut mesh = Mesh::default();
    let mut remap = vec![u32::MAX; unit.len()];
    for j in joints {
        // Positions are rounded to f32 before the inside test (as stored and drawn), so the test sees exactly
        // the vertices that end up on screen.
        let points: Vec<[f32; 3]> = unit.iter().map(|&p| to_f32(j.centre + p * j.radius)).collect();
        let offsets: Vec<DVec3> = points.iter().map(|p| DVec3::from_array(p.map(f64::from)) - j.centre).collect();
        let tolerance = -1e-6 * j.radius;
        let in_a: Vec<bool> = offsets.iter().map(|o| o.dot(j.arm_a) >= tolerance).collect();
        let in_b: Vec<bool> = offsets.iter().map(|o| o.dot(j.arm_b) >= tolerance).collect();
        remap.fill(u32::MAX);
        for face in &template {
            if face.iter().all(|&i| in_a[i as usize]) || face.iter().all(|&i| in_b[i as usize]) {
                continue;
            }
            mesh.faces.push(face.map(|i| {
                if remap[i as usize] == u32::MAX {
                    remap[i as usize] = mesh.vertices.len() as u32;
                    mesh.vertices.push(points[i as usize]);
                }
                remap[i as usize]
            }));
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_only_where_exposed() {
        let tube = |p1, cap0| Segment { p0: DVec3::ZERO, p1, r0: 1.0, r1: 1.0, cap0, cap1: false };
        let mesh = frustums(&[tube(DVec3::Z, true), tube(DVec3::X, false)], 8);
        assert_eq!(mesh.n_faces(), 2 * (2 * 8) + 8); // two open tubes plus one end disc
    }

    #[test]
    fn zero_length_segments_are_dropped() {
        let s = Segment { p0: DVec3::ZERO, p1: DVec3::ZERO, r0: 1.0, r1: 1.0, cap0: true, cap1: true };
        assert_eq!(frustums(&[s], 8).n_faces(), 0);
    }

    #[test]
    fn joint_keeps_only_the_outside_of_the_bend() {
        let bend = Joint { centre: DVec3::ZERO, radius: 1.0, arm_a: DVec3::Z, arm_b: DVec3::X };
        let mesh = joint_spheres(&[bend], 24);
        assert!(mesh.n_faces() > 0);
        for face in &mesh.faces {
            let corners = face.map(|i| mesh.vertices[i as usize]);
            assert!(corners.iter().any(|c| c[2] < 1e-6) && corners.iter().any(|c| c[0] < 1e-6));
        }
        let straight = Joint { centre: DVec3::ZERO, radius: 1.0, arm_a: DVec3::Z, arm_b: -DVec3::Z };
        assert_eq!(joint_spheres(&[straight], 24).n_faces(), 0); // a straight joint is completely covered
    }

    #[test]
    fn unit_sphere_is_closed() {
        let (_, faces) = unit_sphere(24, 12);
        let mut edges = HashMap::new();
        for f in &faces {
            for (a, b) in [(f[0], f[1]), (f[1], f[2]), (f[2], f[0])] {
                *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        assert!(edges.values().all(|&n| n == 2));
    }
}
