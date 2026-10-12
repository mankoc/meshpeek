//! CHITUBOX project files (`.chitubox`): the models of a CHITUBOX scene with their supports.
//!
//! The format is not published; `docs/formats/CHITUBOX_FILE_FORMAT.md` specifies it (reverse-engineered from sample files).
//! In short, all little-endian:
//!
//! - header: magic `0xAB231243`, number of objects, offset of the object table (then the preview and, in newer
//!   files, the support-settings records, which are not needed here);
//! - object table: one 680-byte slot per object with its name, its X/Y position on the plate, its lift above the
//!   plate, and the offsets of its model mesh and of its support block;
//! - meshes: triangles of three float32 corners each (36 bytes, no normals, not indexed), centred on the origin;
//! - support block: the list of support parts (tips, columns, braces, joints, foot pads), each pointing at its own
//!   triangles; together they form the support mesh.

use std::path::Path;

use glam::{DMat4, DVec3};
use rayon::prelude::*;

use crate::error::{LoadError, Result};
use crate::io::map_file;
use crate::mesh::Mesh;
use crate::scene::{ObjectKind, Scene, SceneObject};

/// First four bytes of every `.chitubox` file.
const MAGIC: u32 = 0xAB23_1243;
/// Size of an object's slot in the object table.
const SLOT: usize = 680;
/// Length of the name field at the start of a slot.
const NAME: usize = 256;
/// Bytes per triangle: three corners of three float32.
const TRIANGLE: usize = 36;
/// Offsets inside a slot.
const SLOT_X: usize = 256;
const SLOT_Y: usize = 260;
const SLOT_LIFT: usize = 264;
const SLOT_SUPPORTS: usize = 268;
const SLOT_MESH: usize = 272;
const SLOT_MESH_SIZE: usize = 276;
/// Support-block offset of an object without supports.
const NO_SUPPORTS: u32 = u32::MAX;
/// Size of a support-part entry and the value its first word always holds.
const PART: usize = 72;
const PART_MARKER: u32 = 0xEA34_2389;
/// Offsets inside a part entry: its type, end points, sizes, the byte range of its triangles and the tip depth.
const PART_TYPE: usize = 4;
const PART_A: usize = 8;
const PART_B: usize = 20;
const PART_SIZE_A: usize = 32;
const PART_SIZE_B: usize = 36;
const PART_DEPTH_A: usize = 48;
const PART_MESH: usize = 40;
const PART_MESH_SIZE: usize = 44;
/// Part types that touch the model: tips from above (1) and from below (2).
const CONTACT_TYPES: [u32; 2] = [1, 2];

/// Load a `.chitubox` project.
pub fn load(path: &Path) -> Result<Scene> {
    load_bytes(&map_file(path)?, path)
}

/// Parse a `.chitubox` project already in memory; `path` names the file in errors and in the objects.
pub fn load_bytes(data: &[u8], path: &Path) -> Result<Scene> {
    let (objects, skipped) = objects(data).map_err(|e| LoadError::format(format!("{}: {e}", path.display())))?;
    let mut scene = Scene { skipped, ..Scene::default() };
    for object in objects {
        object.add_to(&mut scene, data, path);
    }
    Ok(scene)
}

/// One object of a project, as stored: where its meshes are and where it stands.
pub struct Object {
    /// Name as CHITUBOX shows it (usually the source file name).
    pub name: String,
    /// Centre of the model on the plate.
    pub x: f64,
    pub y: f64,
    /// Gap between the plate and the model's lowest point.
    pub lift: f64,
    /// Byte range of the model's triangles (read them with [`mesh`]); the model is centred on the origin.
    pub model: (usize, usize),
    /// The supports, when the file has readable ones.
    pub supports: Option<Supports>,
}

/// An object's supports: where their triangles are and the parts they are made of.
pub struct Supports {
    /// Byte range of all support triangles (read them with [`mesh`]), in the model's frame.
    pub range: (usize, usize),
    /// The parts (empty for files whose part list could not be read).
    pub parts: Vec<Part>,
}

/// One support part: a tip, column, brace, joint, foot cone or foot pad (`CHITUBOX_FILE_FORMAT.md`, 6.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// Part type: 1 top tip, 2 bottom tip, 3 column or brace, 4 foot cone, 5 foot pad, 7 contact ball, 9 joint,
    /// 12 link rod.
    pub kind: u32,
    /// End points in the model's frame (for a top tip, `a` touches the model and `b` is below).
    pub a: DVec3,
    pub b: DVec3,
    /// Radius at `a` and at `b` (for foot pads: see the format notes).
    pub size_a: f64,
    pub size_b: f64,
    /// For tips: how deep `a` lies inside the model.
    pub depth_a: f64,
    /// Byte range of this part's triangles (read them with [`mesh`]).
    pub mesh: (usize, usize),
}

/// The objects of a project (in table order) and the objects that could not be read, as `"<name>: <reason>"`.
pub fn objects(data: &[u8]) -> std::result::Result<(Vec<Object>, Vec<String>), String> {
    if word(data, 0) != Some(MAGIC) {
        return Err("not a CHITUBOX project (unknown file signature)".into());
    }
    let count = word(data, 4).unwrap_or(0) as usize;
    let table = word(data, 8).unwrap_or(0) as usize;
    let fits = count.checked_mul(SLOT).and_then(|size| size.checked_add(table)).is_some_and(|end| end <= data.len());
    if count == 0 || !fits {
        return Err("the object table is missing or cut short".into());
    }
    let (mut objects, mut skipped) = (Vec::new(), Vec::new());
    for i in 0..count {
        let slot = &data[table + i * SLOT..table + (i + 1) * SLOT];
        let name = slot_name(slot, i);
        match Object::read(data, slot, name.clone()) {
            Ok(object) => objects.push(object),
            Err(reason) => skipped.push(format!("{name}: {reason}")),
        }
    }
    if objects.is_empty() {
        return Err("no readable objects".into());
    }
    Ok((objects, skipped))
}

impl Object {
    /// Read and check a slot against the file size.
    fn read(data: &[u8], slot: &[u8], name: String) -> std::result::Result<Self, String> {
        let float = |at: usize| f32::from_le_bytes(slot[at..at + 4].try_into().unwrap()) as f64;
        let at = |offset: usize| word(slot, offset).unwrap_or(0) as usize;
        let model = (at(SLOT_MESH), at(SLOT_MESH_SIZE));
        if !triangles_fit(data, model) {
            return Err("its mesh lies outside the file".into());
        }
        let block = word(slot, SLOT_SUPPORTS).unwrap_or(NO_SUPPORTS);
        let supports = (block != NO_SUPPORTS).then(|| supports(data, block as usize, model)).flatten();
        let (x, y, lift) = (float(SLOT_X), float(SLOT_Y), float(SLOT_LIFT));
        if ![x, y, lift].iter().all(|v| v.is_finite()) {
            return Err("its position is not a number".into());
        }
        Ok(Self { name, x, y, lift, model, supports })
    }

    /// Add the model (and its supports, as a child) to `scene`, placed on the plate.
    fn add_to(self, scene: &mut Scene, data: &[u8], path: &Path) {
        let mesh = mesh(data, self.model);
        let bottom = mesh.bounds().map_or(0.0, |(lo, _)| lo[2] as f64);
        let transform = DMat4::from_translation(DVec3::new(self.x, self.y, self.lift - bottom));
        let mut info = vec![("Lift".to_string(), format!("{:.2} mm", self.lift))];
        if let Some(found) = self.supports.as_ref().filter(|s| !s.parts.is_empty()) {
            let contacts = found.parts.iter().filter(|p| CONTACT_TYPES.contains(&p.kind)).count();
            info.push(("Contact points".to_string(), contacts.to_string()));
        }
        let mut model = SceneObject::new(self.name, mesh, path);
        model.transform = transform;
        model.info = info.clone();
        let parent = scene.objects.len();
        scene.objects.push(model);
        if let Some(found) = self.supports {
            let mut supports = SceneObject::new("Supports", self::mesh(data, found.range), path);
            supports.kind = ObjectKind::Support;
            supports.transform = transform;
            supports.parent = Some(parent);
            supports.info = info;
            scene.objects.push(supports);
        }
    }
}

/// The supports described by the support block at `block`: the triangles of all parts of its part list, which
/// lie back to back. Files whose part list cannot be read fall back to the block's support-mesh range, which
/// newer files keep right after the model.
fn supports(data: &[u8], block: usize, model: (usize, usize)) -> Option<Supports> {
    if let Some(found) = part_list(data, block) {
        return Some(found);
    }
    let range = (word(data, block.checked_add(8)?)? as usize, word(data, block.checked_add(12)?)? as usize);
    (range.0 == model.0 + model.1 && triangles_fit(data, range)).then_some(Supports { range, parts: Vec::new() })
}

/// Read the part list of a support block: every entry must carry the part marker and the parts' triangles must
/// follow one another.
fn part_list(data: &[u8], block: usize) -> Option<Supports> {
    let count = word(data, block)? as usize;
    let list = word(data, block.checked_add(4)?)? as usize;
    let end = count.checked_mul(PART)?.checked_add(list)?;
    if count == 0 || end > data.len() {
        return None;
    }
    let (mut start, mut next, mut parts) = (None, 0usize, Vec::with_capacity(count));
    for entry in data[list..end].chunks_exact(PART) {
        if word(entry, 0)? != PART_MARKER {
            return None;
        }
        let (offset, size) = (word(entry, PART_MESH)? as usize, word(entry, PART_MESH_SIZE)? as usize);
        if start.is_some() && offset != next {
            return None;
        }
        start.get_or_insert(offset);
        next = offset.checked_add(size)?;
        let float = |at: usize| f32::from_le_bytes(entry[at..at + 4].try_into().unwrap()) as f64;
        let point = |at: usize| DVec3::new(float(at), float(at + 4), float(at + 8));
        parts.push(Part {
            kind: word(entry, PART_TYPE)?,
            a: point(PART_A),
            b: point(PART_B),
            size_a: float(PART_SIZE_A),
            size_b: float(PART_SIZE_B),
            depth_a: float(PART_DEPTH_A),
            mesh: (offset, size),
        });
    }
    let range = (start?, next - start?);
    triangles_fit(data, range).then_some(Supports { range, parts })
}

/// The preview image as `(width, height, RGB pixels row by row)`, when the file has one
/// (`CHITUBOX_FILE_FORMAT.md`, section 4).
pub fn preview(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let header = word(data, 12)? as usize;
    if header == 0 {
        return None;
    }
    let (width, height) = (word(data, header)?, word(data, header + 4)?);
    let (offset, size) = (word(data, header + 8)? as usize, word(data, header + 12)? as usize);
    let encoded = data.get(offset..offset.checked_add(size)?)?;
    let pixels = (width as usize).checked_mul(height as usize)?;
    let mut rgb = Vec::with_capacity(pixels * 3);
    let mut words = encoded.chunks_exact(2).map(|w| u16::from_le_bytes([w[0], w[1]]));
    while let Some(colour) = words.next() {
        let run = if colour & 0x20 != 0 { (words.next()? & 0x0FFF) as usize + 1 } else { 1 };
        let channel = |shift: u16| ((colour >> shift) & 0x1F) as u32 * 255 / 31;
        let pixel = [channel(11) as u8, channel(6) as u8, channel(0) as u8];
        for _ in 0..run.min(pixels.saturating_sub(rgb.len() / 3)) {
            rgb.extend_from_slice(&pixel);
        }
    }
    (rgb.len() == pixels * 3).then_some((width, height, rgb))
}

/// The object's name: the zero-terminated text at the start of its slot.
fn slot_name(slot: &[u8], index: usize) -> String {
    let field = &slot[..NAME];
    let end = field.iter().position(|&b| b == 0).unwrap_or(NAME);
    let name = String::from_utf8_lossy(&field[..end]).trim().to_string();
    if name.is_empty() { format!("Object {}", index + 1) } else { name }
}

/// A little-endian u32 at `at`, if the data is long enough.
fn word(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at.checked_add(4)?).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// `(offset, size)` holds whole triangles (at least one) inside the file.
fn triangles_fit(data: &[u8], (offset, size): (usize, usize)) -> bool {
    size > 0 && size % TRIANGLE == 0 && offset.checked_add(size).is_some_and(|end| end <= data.len())
}

/// The triangles in `(offset, size)` as an indexed mesh (shared corners merged). The data is not 4-byte
/// aligned in the file, so it is read value by value.
pub fn mesh(data: &[u8], (offset, size): (usize, usize)) -> Mesh {
    let float = |b: &[u8]| f32::from_le_bytes(b.try_into().unwrap());
    let corners: Vec<[f32; 3]> = data[offset..offset + size]
        .par_chunks_exact(12)
        .map(|c| [float(&c[0..4]), float(&c[4..8]), float(&c[8..12])])
        .collect();
    Mesh::from_corners(&corners)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: usize = 64;

    /// One triangle at height `z` (lowest corner), as file bytes.
    fn triangle(z: f32) -> Vec<u8> {
        [[0.0f32, 0.0, z], [1.0, 0.0, z], [0.0, 1.0, z + 2.0]].iter().flatten().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn put(data: &mut [u8], at: usize, value: u32) {
        data[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// A support part entry of `kind` whose triangles are `size` bytes at `offset`.
    fn part(kind: u32, offset: usize, size: usize) -> Vec<u8> {
        let mut entry = vec![0u8; PART];
        put(&mut entry, 0, PART_MARKER);
        put(&mut entry, PART_TYPE, kind);
        put(&mut entry, PART_MESH, offset as u32);
        put(&mut entry, PART_MESH_SIZE, size as u32);
        entry
    }

    /// The header and one object slot (model at `model`, one triangle; support block at `block`).
    fn file(len: usize, model: usize, block: Option<usize>) -> Vec<u8> {
        let mut data = vec![0u8; len];
        put(&mut data, 0, MAGIC);
        put(&mut data, 4, 1);
        put(&mut data, 8, TABLE as u32);
        data[TABLE..TABLE + 5].copy_from_slice(b"thing");
        data[TABLE + SLOT_X..TABLE + SLOT_X + 4].copy_from_slice(&10.0f32.to_le_bytes());
        data[TABLE + SLOT_Y..TABLE + SLOT_Y + 4].copy_from_slice(&(-5.0f32).to_le_bytes());
        data[TABLE + SLOT_LIFT..TABLE + SLOT_LIFT + 4].copy_from_slice(&3.0f32.to_le_bytes());
        put(&mut data, TABLE + SLOT_SUPPORTS, block.map_or(NO_SUPPORTS, |b| b as u32));
        put(&mut data, TABLE + SLOT_MESH, model as u32);
        put(&mut data, TABLE + SLOT_MESH_SIZE, TRIANGLE as u32);
        data
    }

    /// Newer layout: model, support mesh, support block (416 bytes), part list.
    fn newer() -> Vec<u8> {
        let model = TABLE + SLOT;
        let (support, block) = (model + TRIANGLE, model + 3 * TRIANGLE);
        let list = block + 416;
        let mut data = file(list, model, Some(block));
        data[model..model + TRIANGLE].copy_from_slice(&triangle(-1.0));
        data[support..support + 2 * TRIANGLE].copy_from_slice(&[triangle(-4.0), triangle(-3.0)].concat());
        for (k, value) in [2, list, support, 2 * TRIANGLE].into_iter().enumerate() {
            put(&mut data, block + 4 * k, value as u32);
        }
        data.extend(part(1, support, TRIANGLE)); // a tip
        data.extend(part(3, support + TRIANGLE, TRIANGLE)); // a column
        data.extend([0u8; 20]);
        data
    }

    #[test]
    fn model_and_supports_are_placed_on_the_plate() {
        let scene = load_bytes(&newer(), Path::new("t.chitubox")).unwrap();
        assert_eq!(scene.objects.len(), 2);
        let (model, supports) = (&scene.objects[0], &scene.objects[1]);
        assert_eq!(model.name, "thing");
        assert_eq!((supports.kind, supports.parent, supports.mesh.n_faces()), (ObjectKind::Support, Some(0), 2));
        // the model's bottom (z = -1) stands 3 mm above the plate, centred at (10, -5)
        let bottom = model.transform.transform_point3(DVec3::new(0.0, 0.0, -1.0));
        assert_eq!(bottom, DVec3::new(10.0, -5.0, 3.0));
        // the supports reach down to the plate
        assert_eq!(supports.transform.transform_point3(DVec3::new(0.0, 0.0, -4.0)).z, 0.0);
        assert!(model.info.iter().any(|(k, v)| k == "Contact points" && v == "1"));
    }

    #[test]
    fn older_layout_finds_the_supports_through_the_part_list() {
        // block, 20-byte box, part list, part triangles, then the model at the end of the file
        let block = TABLE + SLOT;
        let list = block + 416 + 20;
        let support = list + PART;
        let model = support + TRIANGLE;
        let mut data = file(model + TRIANGLE, model, Some(block));
        data[model..].copy_from_slice(&triangle(-1.0));
        data[support..support + TRIANGLE].copy_from_slice(&triangle(-4.0));
        for (k, value) in [1, list, block + 416, 20].into_iter().enumerate() {
            put(&mut data, block + 4 * k, value as u32);
        }
        data[list..list + PART].copy_from_slice(&part(1, support, TRIANGLE));
        let scene = load_bytes(&data, Path::new("t.chitubox")).unwrap();
        assert_eq!(scene.objects.len(), 2);
        assert_eq!(scene.objects[1].mesh.n_faces(), 1);
    }

    #[test]
    fn object_without_supports() {
        let model = TABLE + SLOT;
        let mut data = file(model, model, None);
        data.extend(triangle(-1.0));
        let scene = load_bytes(&data, Path::new("t.chitubox")).unwrap();
        assert_eq!(scene.objects.len(), 1);
        assert_eq!(scene.objects[0].mesh.n_faces(), 1);
    }

    #[test]
    fn rejects_other_files_and_bad_meshes() {
        assert!(load_bytes(b"solid not a chitubox", Path::new("t.chitubox")).is_err());
        let mut data = newer();
        put(&mut data, TABLE + SLOT_MESH_SIZE, 1000); // the mesh would run past the end of the file
        assert!(load_bytes(&data, Path::new("t.chitubox")).is_err());
    }
}
