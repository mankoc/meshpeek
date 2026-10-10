//! Print the members and decoded scene state of a Lychee `.lys` file.
//!
//!     lys-dump scene.lys                          # members + top-level keys
//!     lys-dump scene.lys objects.present.byId.o1  # one subtree as JSON

use std::path::Path;

use meshpeek_core::io::lys::container::Archive;
use meshpeek_core::io::lys::{codec, value};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.len() > 2 {
        eprintln!("usage: lys-dump <file.lys> [dotted.key.path]");
        std::process::exit(2);
    }
    if let Err(e) = run(Path::new(&args[0]), args.get(1).map(String::as_str)) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(path: &Path, key: Option<&str>) -> meshpeek_core::Result<()> {
    let mut archive = Archive::open(path)?;
    println!("container: {}", if archive.is_zip() { "ZIP (V1)" } else { "MNGO (V2+)" });
    for name in archive.names() {
        let size = archive.read(&name)?.len();
        println!("  {name:<60} {size:>12} bytes");
    }
    let ciphered = !archive.is_zip();
    let state = codec::decode(&archive.read("scene.bin")?, ciphered)?;
    match key {
        None => {
            let keys: Vec<String> =
                state.as_map().into_iter().flatten().filter_map(|(k, _)| k.as_str().map(str::to_string)).collect();
            println!("state keys: {}", keys.join(", "));
        }
        Some(key) => match value::path(&state, key) {
            Some(subtree) => println!("{}", serde_json::to_string_pretty(&value::to_json(subtree)).unwrap()),
            None => eprintln!("no such key: {key}"),
        },
    }
    Ok(())
}
