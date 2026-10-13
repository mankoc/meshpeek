# MeshPeek

Fast viewer for STL, OBJ, PLY, Lychee `.lys` and CHITUBOX `.chitubox` files, written in Rust. Renders on the GPU, or on the CPU
when there is no usable GPU.

## Supported files

| Format | What is shown |
|---|---|
| STL (binary and ASCII), OBJ, PLY | The mesh (PLY and OBJ vertex colours too) |
| Lychee Slicer `.lys` | Every model placed as in Lychee, with its supports, rafts and the build plate |
| CHITUBOX `.chitubox` | Every model where it stands on the plate, with the supports CHITUBOX generated (also from older CHITUBOX versions) |

Models can also be opened from inside ZIP, 7z and RAR archives.

## Run

```
meshpeek [--cpu] [files...]
```

- Files can also be opened with **File → Open** (Ctrl+O), **File → Open Recent**, or dragged onto the window.
- `--cpu` forces the software renderer (WARP on Windows, Mesa lavapipe on Linux). The app picks it
  automatically when no GPU is available. **Help → About** shows which renderer is in use.

## Controls

| Action | Mouse | Keyboard |
|---|---|---|
| Orbit | Left drag | Arrow keys (hold) |
| Pan | Middle drag, or Shift + left drag | Shift + arrow keys |
| Zoom | Mouse wheel, or right drag | + / − or Page Up / Page Down |
| Roll | | Q / E |
| Fit all | | F |
| Standard views | Click an axis of the gizmo (top right) | 0 Isometric, 1 Front, 2 Back, 3 Left, 4 Right, 5 Top, 6 Bottom |
| Wireframe | | W |
| Centre on a model | Crosshair icon next to it in the Objects panel (click again, or F, to stop) | |
| Help | | F1 (Help → Controls); Help → User guide |
| Preferences | | Ctrl+, (materials and colours, support detail, OBJ up axis, space mouse) |

View → Theme switches between System, Light and Dark.

**Space mouse (3Dconnexion):** push/lift the cap to pan, pull/push to zoom, tilt/spin to orbit, twist to roll;
button 1 fits all, button 2 shows the isometric view. Sensitivity and axis inversion are in Preferences.
- Windows: works directly over USB when the 3Dconnexion 3DxWare driver is *not* installed (it would take the
  device exclusively).
- Linux: works through the `spacenavd` daemon if it runs, otherwise directly (the `.deb` installs a udev rule
  that gives the desktop user access).

## Build

Install Rust and Git LFS first (Git LFS is needed for the sample models):

- **Windows 10/11** (PowerShell; Rust links with Microsoft's C++ build tools):
  ```powershell
  winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
  winget install Rustlang.Rustup
  winget install GitHub.GitLFS
  git lfs install
  ```
  Then open a new terminal so `cargo` is on the `PATH`.
- **Linux (Ubuntu/Debian)**:
  ```bash
  sudo apt install build-essential pkg-config libx11-dev libxkbcommon-dev libwayland-dev libgtk-3-dev mesa-vulkan-drivers git-lfs
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  git lfs install
  ```
  `libgtk-3-dev` is for the file dialogs; `mesa-vulkan-drivers` provides the software renderer used in CPU mode.

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
.\scripts\build.ps1                 # checks + MeshPeek-<version>-windows-x64.exe/.zip + MeshPeek-<version>-x86_64.msi
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
| `crates/meshpeek-core` | File formats (STL, OBJ, PLY, `.lys` and `.chitubox` incl. supports), meshes and scenes. No GPU/UI code. |
| `crates/meshpeek-render` | wgpu renderer: GPU meshes, camera, shading, plate, axes. No UI code. |
| `crates/meshpeek-app` | The `meshpeek` desktop app (eframe/egui), icon, installer definitions. |
| `crates/meshpeek-tools` | `meshpeek-info`, `lys-dump`, `meshpeek-bench`, `meshpeek-gen`. |
| `samples/` | Sample models (Git LFS) and the reference data the golden tests compare against. |
| `scripts/` | Build and packaging scripts. |
| `docs/` | Benchmarks, manual test checklist, to-do list. |
| `docs/formats/LYS_FILE_FORMAT.md` | Lychee `.lys` format notes. |
| `docs/formats/CHITUBOX_FILE_FORMAT.md` | CHITUBOX `.chitubox` project format specification (reverse-engineered). |

The original Python implementation is kept on the `python` branch.

## Tools

```
meshpeek-info <file>                 # objects, triangle counts, load time, memory
lys-dump <file.lys> [key.path]       # archive members and decoded Lychee scene state
meshpeek-bench [--cpu] <files...>    # off-screen load/upload/frame-time/memory benchmark
meshpeek-gen <out.stl> <triangles>   # synthetic large STL for testing
```

Run them with `cargo run --release -p meshpeek-tools --bin <tool> -- <args>`.

## License

MeshPeek is licensed under the [Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International
License](https://creativecommons.org/licenses/by-nc-sa/4.0/) (CC BY-NC-SA 4.0); see `LICENSE`. In short: you may
use, share and adapt it for **non-commercial** purposes, with attribution, and adaptations must use the same
license.

MeshPeek includes open-source libraries under their own (permissive) licenses. Their notices are collected in
`THIRD-PARTY-LICENSES.html` by the build scripts (`cargo about`, configured in `about.toml`) and shipped with
every package.
