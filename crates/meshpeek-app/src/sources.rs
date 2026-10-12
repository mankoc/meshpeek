//! Turning what the user picked (files, folders, archives) into model files that can be loaded.
//!
//! An archive is browsed like a folder: its model entries are listed, and a single entry is read into memory
//! when it is opened (nothing is written to disk). An entry has the virtual path `<archive>/<path inside>`.

use std::io::Read;
use std::path::{Path, PathBuf};

use meshpeek_core::SUPPORTED_EXTENSIONS;
use meshpeek_core::io::extension;

/// Archive extensions whose models can be listed and opened.
pub const ARCHIVE_EXTENSIONS: &[&str] = &["zip", "7z", "rar"];

/// `path` names an archive MeshPeek can open.
pub fn is_archive(path: &Path) -> bool {
    ARCHIVE_EXTENSIONS.contains(&extension(path).as_str())
}

/// `path` names a model file MeshPeek can load.
pub fn is_model(path: &Path) -> bool {
    SUPPORTED_EXTENSIONS.contains(&extension(path).as_str())
}

/// One model file to load.
pub enum Source {
    Disk(PathBuf),
    /// A file read from an archive; `name` is `<archive>/<path inside>`.
    Memory {
        name: PathBuf,
        data: Vec<u8>,
    },
}

impl Source {
    /// The file path, or the name inside the archive for unpacked members.
    pub fn path(&self) -> &Path {
        match self {
            Self::Disk(path) | Self::Memory { name: path, .. } => path,
        }
    }
}

/// For a virtual path `<archive>/<entry>`: the archive file and the entry name inside it (with `/`).
pub fn archive_of(path: &Path) -> Option<(PathBuf, String)> {
    if path.exists() {
        return None;
    }
    let archive = path.ancestors().skip(1).find(|a| a.is_file() && is_archive(a))?;
    let entry = path.strip_prefix(archive).ok()?.to_string_lossy().replace('\\', "/");
    Some((archive.to_path_buf(), entry))
}

/// Model files for `paths`: files as they are, folders searched recursively (their models only), entries of
/// archives (virtual paths) read into memory. Problems are added to `problems` as messages.
pub fn expand(paths: impl IntoIterator<Item = PathBuf>, problems: &mut Vec<String>) -> Vec<Source> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            let found = models_in(&path);
            if found.is_empty() {
                problems.push(format!("no model files in {}", path.display()));
            }
            out.extend(found.into_iter().map(Source::Disk));
        } else if let Some((archive, entry)) = archive_of(&path) {
            match read_entry(&archive, &entry) {
                Ok(data) => out.push(Source::Memory { name: path, data }),
                Err(e) => problems.push(format!("{}: {e}", path.display())),
            }
        } else {
            out.push(Source::Disk(path));
        }
    }
    out
}

/// Model files below `dir`, recursively, sorted.
fn models_in(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk(dir, &mut |p| {
        if is_model(p) {
            files.push(p.to_path_buf());
        }
    });
    files.sort();
    files
}

/// Call `visit` for every file under `dir`, recursively.
fn walk(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit);
        } else if is_model(&path) || is_archive(&path) {
            visit(&path);
        }
    }
}

/// An archive member name with forward slashes.
fn normal(name: &str) -> String {
    name.replace('\\', "/")
}

/// The model entries of `archive` (names with `/`), sorted without regard to case.
pub fn list_models(archive: &Path) -> Result<Vec<String>, String> {
    let mut names: Vec<String> = match extension(archive).as_str() {
        "zip" => {
            let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
            let zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            zip.file_names().filter(|n| !n.ends_with('/')).map(normal).collect()
        }
        "7z" => {
            let header = sevenz_rust2::Archive::open(archive).map_err(|e| e.to_string())?;
            header.files.iter().filter(|e| !e.is_directory()).map(|e| normal(e.name())).collect()
        }
        _ => {
            let list = unrar::Archive::new(archive).open_for_listing().map_err(|e| e.to_string())?;
            let mut names = Vec::new();
            for header in list {
                let header = header.map_err(|e| e.to_string())?;
                if header.is_file() {
                    names.push(normal(&header.filename.to_string_lossy()));
                }
            }
            names
        }
    };
    names.retain(|n| is_model(Path::new(n)));
    names.sort_by_key(|n| n.to_lowercase());
    Ok(names)
}

/// The bytes of one entry of `archive`. In a solid 7z or a rar everything before the entry is decoded too.
pub fn read_entry(archive: &Path, entry: &str) -> Result<Vec<u8>, String> {
    let missing = || format!("{entry} not found in the archive");
    match extension(archive).as_str() {
        "zip" => {
            let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            let mut file = zip.by_name(entry).map_err(|e| e.to_string())?;
            let mut data = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut data).map_err(|e| e.to_string())?;
            Ok(data)
        }
        "7z" => {
            let mut reader = sevenz_rust2::ArchiveReader::open(archive, sevenz_rust2::Password::empty())
                .map_err(|e| e.to_string())?;
            let mut found = None;
            reader
                .for_each_entries(|item, content| {
                    if normal(item.name()) == entry {
                        let mut data = Vec::new();
                        content.read_to_end(&mut data)?;
                        found = Some(data);
                        return Ok(false);
                    }
                    std::io::copy(content, &mut std::io::sink())?; // unread entries fail the checksum check
                    Ok(true)
                })
                .map_err(|e| e.to_string())?;
            found.ok_or_else(missing)
        }
        _ => {
            let mut open = unrar::Archive::new(archive).open_for_processing().map_err(|e| e.to_string())?;
            while let Some(header) = open.read_header().map_err(|e| e.to_string())? {
                if normal(&header.entry().filename.to_string_lossy()) == entry {
                    return header.read().map(|(data, _)| data).map_err(|e| e.to_string());
                }
                open = header.skip().map_err(|e| e.to_string())?;
            }
            Err(missing())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn folders_expand_to_their_models() {
        let dir = std::env::temp_dir().join("meshpeek-sources-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.stl"), "x").unwrap();
        std::fs::write(dir.join("sub/b.obj"), "x").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let mut problems = Vec::new();
        assert_eq!(expand([dir], &mut problems).len(), 2);
    }

    #[test]
    fn zip_archives_are_listed_and_read_entry_by_entry() {
        let dir = std::env::temp_dir().join("meshpeek-sources-zip-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("pack.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in [("inner/c.ply", "ply"), ("a.stl", "stl"), ("readme.txt", "txt")] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();

        assert_eq!(list_models(&archive).unwrap(), ["a.stl", "inner/c.ply"]);
        assert_eq!(read_entry(&archive, "inner/c.ply").unwrap(), b"ply");

        let virtual_path = archive.join("inner/c.ply");
        assert_eq!(archive_of(&virtual_path), Some((archive.clone(), "inner/c.ply".to_string())));
        let mut problems = Vec::new();
        let found = expand([virtual_path.clone()], &mut problems);
        assert!(matches!(&found[..], [Source::Memory { name, data }] if *name == virtual_path && data == b"ply"));
    }
}
