//! Turning what the user picked (files, folders, archives) into model files that can be loaded.
//!
//! Archives are read straight into memory (only the model files in them); nothing is written to disk.

use std::io::Read;
use std::path::{Path, PathBuf};

use meshpeek_core::SUPPORTED_EXTENSIONS;
use meshpeek_core::io::extension;

pub const ARCHIVE_EXTENSIONS: &[&str] = &["zip", "7z", "rar"];

pub fn is_archive(path: &Path) -> bool {
    ARCHIVE_EXTENSIONS.contains(&extension(path).as_str())
}

pub fn is_model(path: &Path) -> bool {
    SUPPORTED_EXTENSIONS.contains(&extension(path).as_str())
}

/// One model file to load.
pub enum Source {
    Disk(PathBuf),
    /// A file unpacked from an archive; `name` is `<archive>/<path inside>`.
    Memory {
        name: PathBuf,
        data: Vec<u8>,
    },
}

impl Source {
    pub fn path(&self) -> &Path {
        match self {
            Self::Disk(path) | Self::Memory { name: path, .. } => path,
        }
    }
}

/// Model files for `paths`: files as they are, folders searched recursively, archives read into memory.
/// Problems (unreadable archive, empty folder…) are added to `problems` as messages.
pub fn expand(paths: impl IntoIterator<Item = PathBuf>, problems: &mut Vec<String>) -> Vec<Source> {
    let mut out = Vec::new();
    for path in paths {
        let found = if path.is_dir() {
            let mut found = Vec::new();
            for file in models_in(&path) {
                found.extend(read_one(file, problems));
            }
            found
        } else {
            read_one(path.clone(), problems)
        };
        if found.is_empty() && (path.is_dir() || is_archive(&path)) && problems.is_empty() {
            problems.push(format!("no model files in {}", path.display()));
        }
        out.extend(found);
    }
    out
}

fn read_one(path: PathBuf, problems: &mut Vec<String>) -> Vec<Source> {
    if !is_archive(&path) {
        return vec![Source::Disk(path)];
    }
    match unpack(&path) {
        Ok(found) => found,
        Err(e) => {
            problems.push(format!("{}: {e}", path.display()));
            Vec::new()
        }
    }
}

/// Model files and archives below `dir`, recursively, sorted.
fn models_in(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk(dir, &mut |p| files.push(p.to_path_buf()));
    files.sort();
    files
}

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

/// Read the model files of `archive` into memory.
fn unpack(archive: &Path) -> Result<Vec<Source>, String> {
    let mut found = Vec::new();
    let mut add = |inside: &str, data: Vec<u8>| {
        found.push(Source::Memory { name: archive.join(inside), data });
    };
    match extension(archive).as_str() {
        "zip" => {
            let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
                let name = entry.name().to_string();
                if entry.is_file() && is_model(Path::new(&name)) {
                    let mut data = Vec::with_capacity(entry.size() as usize);
                    entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
                    add(&name, data);
                }
            }
        }
        "7z" => {
            let mut reader = sevenz_rust2::ArchiveReader::open(archive, sevenz_rust2::Password::empty())
                .map_err(|e| e.to_string())?;
            reader
                .for_each_entries(|entry, content| {
                    if !entry.is_directory() && is_model(Path::new(entry.name())) {
                        let mut data = Vec::new();
                        content.read_to_end(&mut data)?;
                        add(entry.name(), data);
                    } else {
                        std::io::copy(content, &mut std::io::sink())?; // unread entries fail the checksum check
                    }
                    Ok(true)
                })
                .map_err(|e| e.to_string())?;
        }
        _ => {
            let mut open = unrar::Archive::new(archive).open_for_processing().map_err(|e| e.to_string())?;
            while let Some(header) = open.read_header().map_err(|e| e.to_string())? {
                let name = header.entry().filename.to_string_lossy().into_owned();
                open = if header.entry().is_file() && is_model(Path::new(&name)) {
                    let (data, next) = header.read().map_err(|e| e.to_string())?;
                    add(&name, data);
                    next
                } else {
                    header.skip().map_err(|e| e.to_string())?
                };
            }
        }
    }
    found.sort_by(|a, b| a.path().cmp(b.path()));
    Ok(found)
}

/// A folder listing for the Files tab: sub-folders first, then files, only what is openable.
pub struct Node {
    pub path: PathBuf,
    pub children: Vec<Node>,
    pub is_dir: bool,
}

/// Tree of `dir` containing only models, archives and the folders that lead to them.
pub fn scan(dir: &Path, depth: usize) -> Node {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    if depth > 0 {
        if let Ok(read) = std::fs::read_dir(dir) {
            for entry in read.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let node = scan(&path, depth - 1);
                    if !node.children.is_empty() {
                        dirs.push(node);
                    }
                } else if is_model(&path) || is_archive(&path) {
                    files.push(Node { path, children: Vec::new(), is_dir: false });
                }
            }
        }
    }
    dirs.sort_by(|a, b| a.path.cmp(&b.path));
    files.sort_by(|a, b| a.path.cmp(&b.path));
    dirs.extend(files);
    Node { path: dir.to_path_buf(), children: dirs, is_dir: true }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn folders_and_zip_archives_expand_to_models() {
        let dir = std::env::temp_dir().join("meshpeek-sources-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.stl"), "x").unwrap();
        std::fs::write(dir.join("sub/b.obj"), "x").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let mut problems = Vec::new();
        assert_eq!(expand([dir.clone()], &mut problems).len(), 2);

        let archive = dir.join("pack.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        zip.start_file("inner/c.ply", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"x").unwrap();
        zip.start_file("readme.txt", zip::write::SimpleFileOptions::default()).unwrap();
        zip.finish().unwrap();
        let found = expand([archive], &mut problems);
        assert_eq!(found.len(), 1);
        assert!(matches!(&found[0], Source::Memory { name, data } if name.ends_with("c.ply") && data == b"x"));
        assert!(problems.is_empty());
    }
}
