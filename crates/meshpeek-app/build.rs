//! Embed the icon and version information into the Windows executable, and give its main thread a bigger stack.

/// Stack of the main thread on Windows (the linker's default is 1 MB; Linux and macOS give 8 MB). The interface,
/// the renderer and the graphics driver (which compiles shaders on that thread) all run there.
const MAIN_STACK: usize = 8 << 20;

/// Embed the application icon and version info in the Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    let target = |key: &str| std::env::var(key).unwrap_or_default();
    if target("CARGO_CFG_TARGET_OS") == "windows" {
        match target("CARGO_CFG_TARGET_ENV").as_str() {
            "msvc" => println!("cargo:rustc-link-arg-bins=/STACK:{MAIN_STACK}"),
            "gnu" => println!("cargo:rustc-link-arg-bins=-Wl,--stack,{MAIN_STACK}"),
            _ => {}
        }
        let mut resources = winresource::WindowsResource::new();
        resources.set_icon("assets/icon.ico");
        resources.set("ProductName", "MeshPeek");
        resources.set("FileDescription", "MeshPeek 3D model viewer");
        resources.set("LegalCopyright", "MeshPeek contributors");
        if let Err(e) = resources.compile() {
            // A missing resource compiler only costs the icon; do not fail the build for it.
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
    }
}
