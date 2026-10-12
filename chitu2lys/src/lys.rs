//! Writing a Lychee `.lys` file: the MNGO container, the ciphered MessagePack scene state and mesh members
//! (`docs/formats/LYS_FILE_FORMAT.md`).

use std::io::Write;
use std::path::Path;

use meshpeek_core::Mesh;
use meshpeek_core::io::lys::codec::FILE_APP_ID;

/// Version written in the container index (what Lychee 7 and 8 write).
const CONTAINER_VERSION: &str = "3.1.0";

/// A file inside the `.lys` container.
pub struct Member {
    pub name: String,
    pub data: Vec<u8>,
}

/// Write `members` as an MNGO container (Electron asar with `mangoFiles` for `files`).
pub fn write(path: &Path, members: &[Member]) -> std::io::Result<()> {
    let mut files = serde_json::Map::new();
    let mut offset = 0usize;
    for member in members {
        files.insert(
            member.name.clone(),
            serde_json::json!({ "size": member.data.len(), "offset": offset.to_string() }),
        );
        offset += member.data.len();
    }
    let index = serde_json::json!({ "version": CONTAINER_VERSION, "mangoFiles": files }).to_string();
    let padded = index.len().div_ceil(4) * 4;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    for word in [4, 8 + padded, 4 + padded, index.len()] {
        out.write_all(&(word as u32).to_le_bytes())?;
    }
    out.write_all(index.as_bytes())?;
    out.write_all(&vec![0u8; padded - index.len()])?;
    for member in members {
        out.write_all(&member.data)?;
    }
    out.flush()
}

/// The scene state as `scene.bin`: MessagePack, ciphered with Lychee's application id.
pub fn scene_bin(state: &serde_json::Value) -> Vec<u8> {
    let mut packed = Vec::new();
    rmpv::encode::write_value(&mut packed, &to_msgpack(state)).expect("writing to memory");
    packed.iter().zip(FILE_APP_ID.iter().cycle()).map(|(&b, &k)| b.wrapping_add(k % 255)).collect()
}

/// A mesh as a `<hash>.bin` member: five u32 (version 2, 12, index count, coordinate count, 0), the indices, the
/// coordinates.
pub fn mesh_bin(mesh: &Mesh) -> Vec<u8> {
    let (indices, coords) = (mesh.faces.len() * 3, mesh.vertices.len() * 3);
    let mut out = Vec::with_capacity(20 + 4 * (indices + coords));
    for word in [2, 12, indices as u32, coords as u32, 0] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    mesh.faces.iter().flatten().for_each(|i| out.extend_from_slice(&i.to_le_bytes()));
    mesh.vertices.iter().flatten().for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    out
}

/// JSON to MessagePack: integers stay integers, other numbers become 64-bit floats.
fn to_msgpack(value: &serde_json::Value) -> rmpv::Value {
    use serde_json::Value as J;
    match value {
        J::Null => rmpv::Value::Nil,
        J::Bool(b) => rmpv::Value::Boolean(*b),
        J::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => rmpv::Value::from(i),
            (_, Some(u)) => rmpv::Value::from(u),
            _ => rmpv::Value::F64(n.as_f64().unwrap_or(0.0)),
        },
        J::String(s) => rmpv::Value::from(s.as_str()),
        J::Array(a) => rmpv::Value::Array(a.iter().map(to_msgpack).collect()),
        J::Object(m) => {
            rmpv::Value::Map(m.iter().map(|(k, v)| (rmpv::Value::from(k.as_str()), to_msgpack(v))).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meshpeek_core::io::Data;
    use meshpeek_core::io::lys::container::Archive;
    use meshpeek_core::io::lys::{codec, mesh_bin as read_mesh, value};

    #[test]
    fn written_files_read_back() {
        let dir = std::env::temp_dir().join("chitu2lys-lys-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.lys");
        let mesh = Mesh::new(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], vec![[0, 1, 2]]);
        let state = serde_json::json!({ "version": 4, "objects": { "n": 1.5, "name": "x" } });
        write(
            &path,
            &[
                Member { name: "abc.bin".into(), data: mesh_bin(&mesh) },
                Member { name: "scene.bin".into(), data: scene_bin(&state) },
            ],
        )
        .unwrap();

        let mut archive = Archive::open(Data::Owned(std::fs::read(&path).unwrap())).unwrap();
        assert!(!archive.is_zip());
        let read = codec::decode(&archive.read("scene.bin").unwrap(), true).unwrap();
        assert_eq!(value::number(value::path(&read, "objects").unwrap(), "n", 0.0), 1.5);
        assert_eq!(value::number(&read, "version", 0.0), 4.0);
        let back = read_mesh::parse(&archive.read("abc.bin").unwrap()).unwrap();
        assert_eq!(back, mesh);
    }
}
