//! Windows Explorer thumbnails for the files MeshPeek opens: a small picture of a file ([`thumbnail`]), the COM
//! provider Explorer asks for it, and the registration of the provider.

mod raster;

#[cfg(windows)]
mod provider;
#[cfg(windows)]
pub mod register;

use std::path::Path;

use meshpeek_core::LoadOptions;
use meshpeek_core::io::preview::{self, preview};
use meshpeek_core::io::{Data, load_data};

/// Largest file whose geometry is loaded for a thumbnail.
pub const MAX_BYTES: u64 = 300 << 20;
/// Largest side of a thumbnail.
pub const MAX_SIZE: u32 = 1024;

/// A picture: RGBA pixels row by row, colours premultiplied by alpha.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

/// The file's own preview picture, if it has one within `data` (possibly only the beginning of the file).
pub fn stored(name: &Path, data: &[u8], size: u32) -> Option<Picture> {
    preview(name, data).map(|picture| shrink(picture, size.clamp(1, MAX_SIZE)))
}

/// A picture of at most `size` × `size` pixels of a file: the preview it stores, or else its geometry drawn.
/// Other files named inside it are never opened.
pub fn thumbnail(name: &Path, data: Vec<u8>, size: u32) -> Option<Picture> {
    let size = size.clamp(1, MAX_SIZE);
    if let Some(picture) = stored(name, &data, size) {
        return Some(picture);
    }
    let options = LoadOptions { support_sides: 6, external_files: false, ..LoadOptions::default() };
    raster::draw(&load_data(name, Data::Owned(data), &options).ok()?, size)
}

/// Scale a picture down to fit `size` × `size` by averaging.
fn shrink((width, height, rgb): preview::Picture, size: u32) -> Picture {
    let longest = width.max(height);
    let fit =
        |side: u32| if longest <= size { side } else { (side as u64 * size as u64 / longest as u64).max(1) as u32 };
    let (w, h) = (fit(width), fit(height));
    let range = |i: u32, small: u32, big: u32| {
        let at = |i: u32| (i as u64 * big as u64 / small as u64) as usize;
        at(i)..at(i + 1)
    };
    let mut pixels = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut count) = ([0u32; 3], 0);
            for row in range(y, h, height) {
                for column in range(x, w, width) {
                    let at = (row * width as usize + column) * 3;
                    (0..3).for_each(|k| sum[k] += rgb[at + k] as u32);
                    count += 1;
                }
            }
            let [r, g, b] = sum.map(|v| (v / count) as u8);
            pixels.push([r, g, b, 255]);
        }
    }
    Picture { width: w, height: h, pixels }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE: &str = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\n\
                        f 4 3 2 1\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n";

    #[test]
    fn a_cube_is_centred_lit_per_side_and_transparent_around() {
        let picture = thumbnail(Path::new("c.obj"), CUBE.into(), 64).unwrap();
        assert_eq!(picture.pixels.len(), 64 * 64);
        let at = |x: usize, y: usize| picture.pixels[y * 64 + x];
        assert_eq!(at(0, 0)[3], 0);
        let (top, left, right) = (at(32, 14), at(20, 40), at(44, 40));
        assert!([top, left, right].iter().all(|p| p[3] == 255));
        assert!(top != left && left != right && top != right, "{top:?} {left:?} {right:?}");
        assert!(picture.pixels.iter().all(|p| p[..3].iter().all(|c| *c <= p[3]))); // premultiplied
    }

    #[test]
    fn broken_files_give_nothing() {
        for name in ["a.stl", "a.obj", "a.ply", "a.3mf", "a.lys", "a.chitubox", "a.ctp", "a.ctb", "a.ags", "a.txt"] {
            assert!(thumbnail(Path::new(name), b"not a model at all".to_vec(), 64).is_none(), "{name}");
            assert!(thumbnail(Path::new(name), Vec::new(), 64).is_none(), "{name}");
        }
        assert!(thumbnail(Path::new("n.obj"), b"v nan 0 0\nv nan 0 0\nv nan 1 0\nf 1 2 3\n".to_vec(), 64).is_none());
    }

    #[test]
    fn previews_shrink_to_fit() {
        let row = [[0u8; 3], [255; 3], [0; 3], [255; 3]].concat();
        let picture = shrink((4, 2, row.repeat(2)), 2);
        assert_eq!((picture.width, picture.height), (2, 1));
        assert_eq!(picture.pixels, vec![[127, 127, 127, 255]; 2]);
    }
}
