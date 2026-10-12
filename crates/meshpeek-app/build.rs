//! Embed the icon and version information into the Windows executable.

/// Embed the application icon and version info in the Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
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
