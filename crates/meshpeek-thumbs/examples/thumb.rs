//! thumb <file> <out.png> [size]: the thumbnail of a file, as Explorer gets it.

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let size = args.get(2).map_or(256, |s| s.parse().expect("size"));
    let data = std::fs::read(&args[0]).expect("file");
    let start = std::time::Instant::now();
    let Some(picture) = meshpeek_thumbs::thumbnail(Path::new(&args[0]), data, size) else {
        eprintln!("{}: no thumbnail", args[0]);
        std::process::exit(1);
    };
    println!("{}: {} x {} in {:?}", args[0], picture.width, picture.height, start.elapsed());
    let bytes = picture.pixels.concat();
    image::save_buffer(&args[1], &bytes, picture.width, picture.height, image::ColorType::Rgba8).expect("png");
}
