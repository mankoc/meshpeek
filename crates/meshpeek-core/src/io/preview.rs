//! The preview pictures stored in CHITUBOX files, for showing a file without loading its geometry.

use std::path::Path;

use super::{ctb, extension};

/// Most pixels a preview may have: a broken header must not ask for all memory.
const MAX_PIXELS: usize = 32 << 20;

/// A picture: width, height and RGB pixels row by row.
pub type Picture = (u32, u32, Vec<u8>);

/// The preview picture of a file, if its format stores one that lies within `data`, which may be only the beginning
/// of the file; `name` picks the format.
pub fn preview(name: &Path, data: &[u8]) -> Option<Picture> {
    let wide = |at: usize| usize::try_from(u64::from_le_bytes(data.get(at..at.checked_add(8)?)?.try_into().ok()?)).ok();
    let header = match extension(name).as_str() {
        "chitubox" if word(data, 0)? == 0xAB23_1243 => word(data, 12)? as usize,
        "ctp" if word(data, 0)? == 0xAB23_1253 => {
            let at = wide(48)?;
            let (offset, size) = (wide(at.checked_add(8)?)?, wide(at.checked_add(16)?)?);
            return decode(data.get(offset..offset.checked_add(size)?)?, word(data, at)?, word(data, at + 4)?);
        }
        "ctb" => ctb::preview_offset(data)?,
        _ => return None,
    };
    if header == 0 {
        return None;
    }
    let (offset, size) = (word(data, header.checked_add(8)?)? as usize, word(data, header.checked_add(12)?)? as usize);
    decode(data.get(offset..offset.checked_add(size)?)?, word(data, header)?, word(data, header + 4)?)
}

/// A little-endian u32 at `at`, if the data is long enough.
fn word(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at.checked_add(4)?).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// Undo the 15-bit run-length encoding (`CHITUBOX_FILE_FORMAT.md`, section 4.2).
fn decode(encoded: &[u8], width: u32, height: u32) -> Option<Picture> {
    let pixels = (width as usize).checked_mul(height as usize).filter(|n| (1..=MAX_PIXELS).contains(n))?;
    let mut rgb = Vec::with_capacity(pixels * 3);
    let mut words = encoded.as_chunks::<2>().0.iter().map(|w| u16::from_le_bytes(*w));
    while let Some(colour) = words.next() {
        let run = if colour & 0x20 != 0 { (words.next()? & 0x0FFF) as usize + 1 } else { 1 };
        let channel = |shift: u16| (((colour >> shift) & 0x1F) as u32 * 255 / 31) as u8;
        let pixel = [channel(11), channel(6), channel(0)];
        for _ in 0..run.min(pixels - rgb.len() / 3) {
            rgb.extend_from_slice(&pixel);
        }
    }
    (rgb.len() == pixels * 3).then_some((width, height, rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chitubox_preview_is_decoded_and_broken_ones_are_refused() {
        let mut data = vec![0u8; 64];
        data[..4].copy_from_slice(&0xAB23_1243u32.to_le_bytes());
        data[12..16].copy_from_slice(&16u32.to_le_bytes());
        for (at, value) in [(16, 2u32), (20, 1), (24, 60), (28, 4)] {
            data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        data[60..62].copy_from_slice(&0xFFFFu16.to_le_bytes()); // white, a run of two
        data[62..64].copy_from_slice(&0x3001u16.to_le_bytes());
        let name = Path::new("a.chitubox");
        assert_eq!(preview(name, &data), Some((2, 1, vec![255; 6])));
        assert_eq!(preview(name, &data[..62]), None); // cut short
        data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(preview(name, &data), None); // absurd size
        assert_eq!(preview(Path::new("a.stl"), &data), None);
    }
}
