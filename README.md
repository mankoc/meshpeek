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

Install Rust first (see Appendix A of `docs/RUST_MIGRATION_PLAN.md`; Git LFS is needed for the sample models).

Day-to-day development:

```
cargo run --release -p meshpeek-app -- samples/Pilone_DX.obj
cargo test --all
```

Packages are built with the scripts in `scripts/` (the GitHub workflow uses the same scripts). Output goes to
`dist/`.

**Windows** (PowerShell):

```powershell
.\scripts\build.ps1 -InstallTools   # first time only: installs WiX Toolset 3 and cargo-wix
.\scripts\build.ps1                 # checks + MeshPeek-<version>-windows-x64.zip + MeshPeek-<version>-x86_64.msi
.\scripts\build.ps1 -SkipTests      # same, without format/lint/test checks
.\scripts\build.ps1 -Check          # only the checks
```

**Linux** (Ubuntu/Debian):

```bash
scripts/build.sh --install-tools      # first time only: apt build dependencies and cargo-deb
scripts/build.sh                      # checks + MeshPeek-<version>-linux-x64.tar.gz + meshpeek_<version>-1_amd64.deb
scripts/build.sh --skip-tests         # same, without checks
scripts/build.sh --check              # only the checks
```

The Windows `meshpeek.exe` is self-contained (static C runtime): the zip can be unpacked and run anywhere on
Windows 10 1909 or newer. The MSI installs it under Program Files with a Start-menu entry. The Linux `.deb`
depends on `mesa-vulkan-drivers` for the CPU fallback.

On GitHub: Actions → **Build packages** → **Run workflow** (manual only) builds all four packages on clean
Windows and Ubuntu machines and attaches them to the run. Tick *publish* to also create a draft release tagged
`v<version>`; raise `version` in `Cargo.toml` first.

## Project layout

| Path | Purpose |
|---|---|
| `crates/meshpeek-core` | File formats (STL, OBJ, PLY, `.lys` incl. supports), meshes and scenes. No GPU/UI code. |
| `crates/meshpeek-render` | wgpu renderer: GPU meshes, camera, shading, plate, axes. No UI code. |
| `crates/meshpeek-app` | The `meshpeek` desktop app (eframe/egui), icon, installer definitions. |
| `crates/meshpeek-tools` | `meshpeek-info`, `lys-dump`, `meshpeek-bench`, `meshpeek-gen`. |
| `samples/` | Sample models (Git LFS) and the reference data the golden tests compare against. |
| `scripts/` | Build and packaging scripts. |
| `docs/` | Migration plan, benchmarks, manual test checklist. |
| `LYS_FILE_FORMAT.md` | Lychee `.lys` format notes. |

The original Python implementation is kept on the `python` branch.

## Tools

```
meshpeek-info <file>                 # objects, triangle counts, load time, memory
lys-dump <file.lys> [key.path]       # archive members and decoded Lychee scene state
meshpeek-bench [--cpu] <files...>    # off-screen load/upload/frame-time/memory benchmark
meshpeek-gen <out.stl> <triangles>   # synthetic large STL for testing
```

Run them with `cargo run --release -p meshpeek-tools --bin <tool> -- <args>`.
