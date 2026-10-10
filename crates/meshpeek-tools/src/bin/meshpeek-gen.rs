//! Write a synthetic binary STL (a finely tessellated torus) for large-model testing.
//!
//!     meshpeek-gen <out.stl> <triangles>

use std::io::{BufWriter, Write};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(out), Some(triangles)) = (args.first(), args.get(1).and_then(|n| n.parse::<u64>().ok())) else {
        eprintln!("usage: meshpeek-gen <out.stl> <triangles>");
        std::process::exit(2);
    };
    // A torus grid of u x v quads = 2uv triangles, with u = 2v.
    let v = ((triangles as f64 / 4.0).sqrt().ceil() as u64).max(3);
    let u = 2 * v;
    let (big, small) = (60.0f64, 20.0f64);
    let point = |i: u64, j: u64| {
        let (a, b) = (std::f64::consts::TAU * i as f64 / u as f64, std::f64::consts::TAU * j as f64 / v as f64);
        let r = big + small * b.cos();
        [(r * a.cos()) as f32, (r * a.sin()) as f32, (small * b.sin() + small) as f32]
    };
    let mut file = BufWriter::with_capacity(1 << 22, std::fs::File::create(out)?);
    let mut header = [b' '; 80];
    header[..19].copy_from_slice(b"meshpeek-gen torus ");
    file.write_all(&header)?;
    file.write_all(&((2 * u * v) as u32).to_le_bytes())?;
    for i in 0..u {
        for j in 0..v {
            let (p00, p10, p01, p11) = (point(i, j), point(i + 1, j), point(i, j + 1), point(i + 1, j + 1));
            for tri in [[p00, p10, p11], [p00, p11, p01]] {
                file.write_all(&[0u8; 12])?;
                for p in tri {
                    p.iter().try_for_each(|c| file.write_all(&c.to_le_bytes()))?;
                }
                file.write_all(&[0u8; 2])?;
            }
        }
    }
    file.flush()?;
    println!("wrote {out}: {} triangles", 2 * u * v);
    Ok(())
}
