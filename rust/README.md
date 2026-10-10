# MeshPeek

Fast viewer for STL, OBJ, PLY and Lychee `.lys` files, written in Rust. Renders on the GPU, or on the CPU
when there is no usable GPU.

## Run

```
meshpeek [--cpu] [files...]
```

- Files can also be opened with **File → Open** (Ctrl+O), **File → Open Recent**, or dragged onto the window.
- `--cpu` forces the software renderer (WARP on Windows, Mesa lavapipe on Linux). The app picks it
  automatically when no GPU is available. **Help → About** shows which renderer is in use.

## Controls

| Action | Input |
|---|---|
| Orbit | Left drag |
| Pan | Middle drag, or Shift + left drag |
| Zoom | Mouse wheel, or right drag |
| Fit all | F |
| Standard views | 0 Isometric, 1 Front, 2 Back, 3 Left, 4 Right, 5 Top, 6 Bottom, or click an axis of the gizmo (top right) |
| Wireframe | W |
| Centre on an object | Right-click it in the Objects panel → *Center on object*; click the 🎯 icon to stop |
| Preferences | Ctrl+, (materials and colours for models and supports, support detail, OBJ up axis) |

## Build

Install Rust first (see Appendix A of `docs/RUST_MIGRATION_PLAN.md`). Then, from this folder:

```
cargo run --release -p meshpeek-app -- path/to/model.stl
cargo test --all
```

Workspace crates:

| Crate | Purpose |
|---|---|
| `meshpeek-core` | File formats (STL, OBJ, PLY, `.lys` incl. supports), meshes and scenes. No GPU/UI code. |
| `meshpeek-render` | wgpu renderer: GPU meshes, camera, shading, plate, axes. No UI code. |
| `meshpeek-app` | The `meshpeek` desktop app (eframe/egui). |
| `meshpeek-tools` | `meshpeek-info`, `lys-dump`, `meshpeek-bench`, `meshpeek-gen`. |

## Tools

```
meshpeek-info <file>                 # objects, triangle counts, load time, memory
lys-dump <file.lys> [key.path]       # archive members and decoded Lychee scene state
meshpeek-bench [--cpu] <files...>    # off-screen load/upload/frame-time/memory benchmark
meshpeek-gen <out.stl> <triangles>   # synthetic large STL for testing
```

## Installers

The `Release` GitHub workflow builds them; push a tag like `v0.1.0` to publish a release.

- **Windows:** MSI built with `cargo wix -p meshpeek-app` (needs WiX Toolset 3).
- **Linux:** `.deb` built with `cargo deb -p meshpeek-app`. It depends on `mesa-vulkan-drivers` for the CPU
  fallback.
