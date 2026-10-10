# MeshPeek — Rust migration plan

*MeshPeek is the new product name (the Python app was a placeholder "STL Viewer").*

Branch: `rust_migration` (from `main` @ `3d868ee`). The Python version is preserved on branch `python`.

## Status (2026-10-10)

| Phase | State | Evidence |
|---|---|---|
| 0 Setup | Done | Rust workspace, `.gitignore`, samples in Git LFS, Python reference dumps, CI workflow |
| 1 `meshpeek-core` | Done | All formats ported; golden tests match the Python reference for all 6 cases |
| 2 Renderer + viewer | Done | wgpu renderer with VTK-matching shading; runs on RTX 3070 and on WARP (`--cpu`) |
| 3 App parity | Done | Every item of §5 implemented; interactive checks listed in `docs/manual-test.md` |
| 4 Gizmo, large models | Done | Clickable gizmo; 30M-triangle STL at 5.9 ms/frame on GPU; chunked buffers; CPU copies freed |
| 5 Packaging | Configured | Icon + version info in the exe; MSI (WiX) and `.deb` built by `.github/workflows/release.yml` — **not yet run** (no WiX locally; needs a push) |
| Cut-over | Pending | Remove Python from `main` once the installers are verified |

Results: `docs/benchmarks.md`. Open follow-up: reduced-detail rendering while orbiting very large models in
CPU-only mode (2 fps at 30M triangles on WARP).

## 1. Goals and how we will measure them

| Goal | Target | How it is checked |
|---|---|---|
| Portability | Windows 10/11 and Linux (x86-64), GPU **or** CPU-only | CI builds both; manual run with the software adapter forced (`--cpu`) |
| Speed | Open + first frame faster than Python; ≥ 60 fps while orbiting 10M triangles on a mid-range GPU | `meshpeek-bench` harness (load time, frame time) vs. the Python numbers below |
| Memory | Process total well under Python's; no fixed 600 MB runtime baseline | Peak/private memory reported by the bench harness |
| Very large models | Open models whose GPU data exceeds a single GPU buffer limit (≥ 256 MB) | Synthetic 30M-triangle STL in the bench suite |
| Deployment | One installer per OS, one executable inside | `cargo-dist` release artifacts from CI |
| Maintainability | Small, well-separated crates; tests for every format quirk | `cargo test`, `clippy -D warnings`, `rustfmt` in CI |

**Python baseline to beat** (measured on this machine, RTX 3070, bust `.lys` + pilone OBJ + benchy STL = 3.3M triangles, 1600×1000):

| Metric | Python |
|---|---|
| Load (all three files) | 1.7 s |
| Process memory, total | 1115 MB (data 94 MB, scene +420 MB, ~600 MB runtime) |
| Frame time | 3.7–3.8 ms |
| Pilone OBJ (85 MB, 1.06M tris) alone | ~1.5 s |
| Benchy STL (225k tris) | 76 ms |

## 2. Technology choices

| Concern | Choice | Why |
|---|---|---|
| UI | **egui** via **eframe** (wgpu backend) | Immediate-mode UI covers everything we have: menus, toolbar, tree with checkboxes and context menus, dialogs, status bar. Pure Rust, no Qt to deploy. |
| 3D rendering | **wgpu**, our own small renderer, drawn inside egui through `egui_wgpu` paint callbacks | wgpu runs on DX12/Vulkan/Metal/GL **and** has a software fallback: WARP on Windows (built into the OS), lavapipe/llvmpipe (Mesa) on Linux — one flag. Gives us exact control over GPU memory and buffer layout. |
| Math | `glam` | Fast, small, SIMD vectors/matrices. |
| Parallelism | `rayon` | Parallel parsing/dedup/support generation. |
| File access | `memmap2` | Map files instead of reading them into memory. |
| `.lys` | `zip` (V1), own MNGO reader (V2), `rmpv` (MessagePack → dynamic value), `serde_json` (MNGO index) | Mirrors the Python decoding exactly. |
| PLY | `ply-rs` (or a small own reader if it is slow) | Binary + ASCII, vertex colours. |
| OBJ | Own reader (port of `core/io/obj.py`) using `memchr` | Fast, tolerant of Blender quirks (empty `usemtl`), keeps shared vertices. |
| File dialogs | `rfd` | Native dialogs on both OSes. |
| Settings & recent files | `serde` + `toml` in the OS config dir (`directories`) | Human-readable, replaces QSettings/registry. |
| Errors / logging | `thiserror` (core), `anyhow` (app), `log` + `env_logger` | |
| Packaging | `cargo-dist`: MSI (Windows), `.deb` (Linux) | One command in CI produces both installers. |

> Decided: **wgpu** rather than three-d. three-d is OpenGL-only, so CPU-only Windows would need a bundled Mesa `opengl32.dll`; wgpu has WARP built in, integrates natively with egui, and gives full control over GPU memory for very large models. Cost: we write a small renderer ourselves (~1,500–2,500 lines).

## 3. Architecture

Cargo workspace in `rust/` (Python code stays at the root until parity, then moves away):

```
rust/
├── Cargo.toml                     # workspace
├── crates/
│   ├── meshpeek-core/             # no GPU/UI deps: formats, mesh, scene, supports
│   │   └── src/{mesh.rs, scene.rs, io/{mod.rs, stl.rs, obj.rs, ply.rs, lys/{container.rs, codec.rs, mesh_bin.rs, loader.rs, supports.rs}}}
│   ├── meshpeek-render/           # wgpu renderer: GPU meshes, camera, pipelines, gizmo
│   ├── meshpeek-app/              # eframe/egui application (binary: meshpeek)
│   └── meshpeek-tools/            # binaries: lys-dump, meshpeek-bench, meshpeek-info
└── tests/data -> ../../samples     # Git LFS sample models (Phase 0)
```

Dependency direction: `app → render → core`, `tools → core (+render for bench)`. `meshpeek-core` stays free of GPU/UI code, exactly like `stl_viewer/core` today.

### 3.1 Python → Rust module map

| Python | Rust | Notes |
|---|---|---|
| `core/mesh.py` (`Mesh`, `from_corners`, `merge`, `FACE_DTYPE`) | `meshpeek-core::mesh` | `Vec<[f32;3]>`, `Vec<[u32;3]>`, `Option<Vec<[u8;3]>>`; dedup with a parallel sort on the float bit patterns (same algorithm). |
| `core/scene.py` | `meshpeek-core::scene` | `SceneObject { name, mesh: Arc<Mesh>, transform: Mat4, kind, visible, source, parent: Option<ObjectId> }` — parent by id, not pointer. |
| `core/io/registry.py` (`LoadOptions`, `load`, `load_mesh`, `Y_UP_TO_Z_UP`) | `meshpeek-core::io` | Same API shape; `LoadOptions { support_sides, obj_y_up }`. |
| `core/io/stl.py` | `io::stl` | mmap; binary size check before the `solid` test; ASCII parser. |
| `core/io/obj.py` | `io::obj` | Port of the byte-level parser (v / f, v/vt/vn, negatives, tabs, fan triangulation, vertex colours). Parallel over line chunks. |
| `core/io/vtk_formats.py` | `io::ply` (+ ASCII STL in `io::stl`) | VTK is no longer needed for loading. |
| `core/io/lys/*` | `io::lys::*` | 1:1 port: MNGO index, cipher (`a8ee1146-…`), msgpack, 20-byte mesh header, object matrix (position.z = lowest point, `center` ignored, Euler XYZ), `.lyd` skipped. |
| `core/io/lys/supports.py` | `io::lys::supports` | Feet/knees/branches/braces, caps only where exposed, joint spheres culled to the outer wedge, `JOINT_SCALE`; generated in parallel per model. |
| `gui/viewport.py` | `meshpeek-render` | GPU meshes, materials, plate, centring/focus. |
| `gui/shadows.py` (+ planar shadows in `viewport.py`) | — | **Dropped:** shadows were an experiment that did not improve visibility enough. Still available on branch `python`. |
| `gui/main_window.py`, `settings*.py`, `app.py` | `meshpeek-app` | egui panels, dialogs, settings, recent files, background loading. |
| `tools/lys_dump.py` | `meshpeek-tools::lys-dump` | Same CLI. |
| `tests/*.py` | `#[test]`s + integration tests | Every Python test gets a Rust counterpart (§7). |

## 4. Key design decisions

### 4.1 Memory model (large-model focus)
- **One copy of the geometry.** Parsed arrays are uploaded to the GPU and then **dropped from RAM** by default (keep only bounds, counts and a reload path). Python kept CPU + VTK copies.
- **32-bit indices**, float32 positions, **no normal buffer**: flat shading computes the face normal in the fragment shader from screen-space derivatives (what VTK did for us). Per vertex: 12 bytes (+3 for colours).
- **Chunked GPU buffers:** meshes are split into chunks below `max_buffer_size` / `max_storage_buffer_binding_size` (often 256 MB), so models larger than one buffer still load.
- **Streaming upload:** binary STL/`.lys` meshes are parsed from the mmap and uploaded chunk by chunk, keeping peak RAM close to one chunk instead of the whole model where the format allows it (STL dedup needs the whole corner list; it is done in parallel and freed right after).
- Optional later: `meshopt` simplification for a level-of-detail while orbiting very large scenes (Phase 4, only if the frame-time target is missed).

### 4.2 Rendering
- Pipelines: opaque mesh (per-object uniform: model matrix, colour, material params; optional vertex colours), plate (translucent quad + outline lines), axes, orientation gizmo.
- Lighting: headlight + key/fill lights approximating VTK's light kit; material presets (Matte, Plastic, Glossy, Metal) map to the same ambient/diffuse/specular/power values as `gui/settings.py:MATERIALS`.
- **MSAA 4×** (falls back to 1× on adapters that do not support it, e.g. some software paths).
- Camera: trackball orbit / pan / zoom matching VTK's interaction style; view presets (0–6), fit all, centre on object with sticky focus (port of `Viewport.center_on/show_view/_frame_focus`).
- **Static scene = redraw on demand only** (egui repaints on input), so idle CPU/GPU use is ~0.

### 4.3 Loading & threading
- Each file loads on a worker thread (rayon inside); results come back over a channel; GPU upload happens on the UI thread in chunks spread across frames so the UI never freezes; status bar shows progress and timing.
- Reload on load-option change (support detail, OBJ Y-up), as now.

### 4.4 CPU-only mode
- At start, wgpu requests a high-performance adapter; if none (or `--cpu` / setting "Force software rendering"), it requests the fallback adapter (WARP on Windows, lavapipe/llvmpipe on Linux).
- The status bar/About box shows the active adapter (answers "is it using the GPU?").
- The `.deb` declares the Mesa Vulkan drivers (`mesa-vulkan-drivers`) as a dependency, so the CPU path works after install.

## 5. Feature parity checklist (from the Python app)

Formats & loading
- [ ] Binary/ASCII STL (size check, vertex dedup), OBJ (Blender quirks, polygons, vertex colours), PLY (vertex colours)
- [ ] `.lys` V1 ZIP + V2 MNGO, cipher + msgpack, embedded meshes, `libraryFile` external STL, `.lyd` skipped and listed
- [ ] Lychee placement (position.z = lowest point, Euler XYZ), build plate, plate dedup
- [ ] Supports: feet, knees, branches, braces (`parentTipId`), joint spheres (culled), exposed caps only, detail Low/Medium/High/Very high
- [ ] OBJ Y-up → Z-up option
- [ ] Background loading, parallel files, status bar timing, error messages

Viewer
- [ ] Orbit/pan/zoom, view presets 0–6, Fit all (F), wireframe (W)
- [ ] Orientation gizmo (clickable cube/axes to snap views) — custom, since VTK's widget goes away
- [ ] Origin axes, plate floor + build-volume outline
- [ ] Materials and colours for models and supports, file vertex colours
- [ ] Centre on object (sticky focus, orbit pivot), target icon in the tree, click icon to release

UI
- [ ] Menus: File (Open, Open Recent ×10 with missing files greyed + Clear list, Clear scene, Exit), View, Settings
- [ ] Toolbar: Open, Clear scene
- [ ] Objects tree: models with "Supports" children, visibility checkboxes, skipped-file warnings, tooltips (path, triangles), right-click "Center on object"
- [ ] Preferences: model material/colour, support material/colour, support detail, OBJ Y-up; live preview + Cancel restores
- [ ] Drag & drop, command-line files, settings persisted

## 6. Phases

Each phase ends with a usable, reviewable build and its acceptance criteria. Estimates assume one developer working with Claude; they are effort, not calendar time.

### Phase 0 — Setup (½–1 day)
- Install toolchain: see **Appendix A** (Build Tools, rustup, Git LFS, cargo-dist).
- Create `rust/` workspace and the four crates; `rustfmt.toml`, `clippy` config.
- Repo hygiene: add `.gitignore` (`__pycache__/`, `target/`, `.venv/`, `.pytest_cache/`), remove committed `__pycache__` files; move sample models to `samples/` and track them with **Git LFS** (85 MB OBJ is near GitHub's 100 MB limit).
- **Reference dumps:** a Python script (`tools/dump_reference.py`) writes, for every sample file, a JSON with per-object name, kind, parent, vertex/face counts, bounds, transform matrix, a hash of sorted triangles, support triangle counts per detail level. These are the golden files for the Rust port.
- GitHub Actions: Windows + Ubuntu, `cargo fmt --check`, `clippy -D warnings`, `cargo test`.
- **Acceptance:** CI green on an empty workspace; reference JSON committed.

### Phase 1 — `meshpeek-core` (formats) (4–6 days)
- Port mesh, scene, STL, OBJ, PLY, `.lys` (container, codec, mesh_bin, loader, supports), registry/LoadOptions.
- `lys-dump` and `meshpeek-info` CLIs (`meshpeek-info file` prints objects, counts, bounds, load time, peak memory).
- Port all Python tests; add golden tests against the reference JSON; property tests for the OBJ parser and STL dedup (`proptest`).
- Benchmarks (`criterion`): each sample file, plus a generated 30M-triangle STL.
- **Acceptance:** all golden tests match Python exactly (counts, bounds, matrices; triangle hashes); every format loads at least as fast as Python with lower peak memory, numbers recorded in `docs/benchmarks.md`.

### Phase 2 — Renderer + minimal viewer (5–8 days)
- `meshpeek-render`: device/adapter selection (GPU → fallback), chunked mesh upload, mesh/plate/axes pipelines, materials, MSAA, trackball camera, view presets, fit, wireframe.
- `meshpeek-app` skeleton: eframe window, central 3D view, File → Open, drag & drop, command line, status bar, background loading, adapter shown in About.
- `meshpeek-bench`: off-screen render timing + memory, same scene as the Python benchmark.
- **Acceptance:** opens all sample files and renders them like the Python viewer (side-by-side screenshots); orbiting is smooth; runs with `--cpu`; bench beats the Python frame time and memory figures.

### Phase 3 — Application parity (5–7 days)
- Objects tree (children, checkboxes, warnings, tooltips, context menu, centring + target icon), toolbar, menus, recent files, preferences dialog with live preview/cancel, settings persistence, reload on load-option change, sticky focus behaviour.
- **Acceptance:** every UI item of §5 works; a manual test script (`docs/manual-test.md`) passes on Windows and Linux.

### Phase 4 — Orientation gizmo, very large models (4–6 days)
- Orientation gizmo, chunked upload across frames with progress, CPU-copy dropping, memory/frame-time tuning; optional `meshopt` LOD if the 10M-triangle target is missed.
- **Acceptance:** gizmo snaps to all six views and isometric; 30M-triangle file opens and orbits on the RTX 3070 and on WARP (slow but usable); memory figures recorded.

### Phase 5 — Packaging, docs, cut-over (2–3 days)
- `cargo-dist`: MSI for Windows (WiX), `.deb` for Linux; app icon, version info, file associations for `.stl/.obj/.ply/.lys` (optional).
- README (build, run, install), `docs/benchmarks.md` final numbers, `LYS_FILE_FORMAT.md` updated with our findings (mesh header, transforms, supports, braces).
- Cut-over: Rust app becomes the root project; Python removed from `main` (kept on branch `python`).
- **Acceptance:** fresh Windows VM and fresh Ubuntu VM: install from the release artifact, open every sample, GPU and CPU-only.

**Total: roughly 4–5 weeks of effort**, front-loaded with low risk (Phase 1 is a direct port with golden tests).

## 7. Testing strategy
- **Unit tests** per module (ported 1:1 from `tests/test_io.py`, `test_settings.py`, `test_viewport.py`).
- **Golden tests** vs. Python reference JSON (Phase 0) for every sample file and detail level.
- **Property tests:** OBJ corner formats/whitespace, STL dedup round-trip (`from_corners` preserves every triangle).
- **Render tests:** off-screen render of fixed scenes compared with stored images (tolerance-based) on CI (Linux runner with lavapipe), covering materials, supports and the gizmo.
- **Benchmarks** tracked in `docs/benchmarks.md` per phase; regressions reviewed.

## 8. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Rust learning curve / maintainability | Slower changes | Small crates, clear module map (§3.1), doc comments carried over from Python, clippy-clean code, CI. |
| egui lacks a ready-made tree/gizmo widget | Extra UI work | Tree via `egui::CollapsingHeader` + checkboxes (simple); gizmo is a small custom render pass. |
| Software rendering (WARP/llvmpipe) is slow on huge models | Poor CPU-only experience | Auto-LOD while orbiting (Phase 4), MSAA off on software adapters. |
| Linux CPU-only depends on Mesa being present | Fails to start on minimal systems | `.deb` depends on `mesa-vulkan-drivers`; clear error message if no adapter is found. |
| `.lys` format changes in new Lychee versions | Load failures | Golden tests, version checks, `lys-dump` for quick investigation (as today). |
| GPU buffer limits differ per adapter | Large models fail on some GPUs | Query limits at start-up and size chunks from them. |
| Large sample files in git | Repo bloat, 100 MB GitHub limit | Git LFS (Phase 0). |

## 9. Decisions and open questions

| # | Topic | Status |
|---|---|---|
| 1 | Renderer: wgpu instead of three-d | **Decided:** wgpu (see §2) |
| 2 | Linux packaging | **Decided:** `.deb` only |
| 3 | Minimum Windows version 10 1909+ (WARP via DX12) | **Decided:** yes |
| 4 | Sample files to `samples/` under Git LFS | **Decided:** yes |
| 5 | Product, crate and executable names | **Decided:** **MeshPeek** — crates `meshpeek-core/-render/-app/-tools`, executable and package `meshpeek` |
| 6 | macOS | **Decided:** out of scope (wgpu keeps a later port cheap) |

## Appendix A — Installing Rust (Phase 0)

### Windows 10/11
Run these in PowerShell. `winget` is built into current Windows; each step also has a manual download.

1. **C++ build tools** (Rust on Windows links with Microsoft's linker):
   ```powershell
   winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```
   Manual: download "Build Tools for Visual Studio 2022" from https://visualstudio.microsoft.com/downloads/ and tick **Desktop development with C++**. If Visual Studio 2022 with that workload is already installed, skip this step.
2. **Rust** (rustup manages the compiler and updates):
   ```powershell
   winget install Rustlang.Rustup
   ```
   Manual: run `rustup-init.exe` from https://rustup.rs and accept the default (`stable-x86_64-pc-windows-msvc`).
3. **Open a new terminal** (so `PATH` is updated) and check:
   ```powershell
   rustc --version
   cargo --version
   rustup component add clippy rustfmt   # usually already included
   ```
4. **Git LFS** (for the sample models):
   ```powershell
   winget install GitHub.GitLFS
   git lfs install
   ```
5. **Packaging tool** (needed from Phase 5; installing early is fine):
   ```powershell
   cargo install cargo-dist --locked
   ```

### Linux (Ubuntu/Debian)
```bash
sudo apt install build-essential pkg-config libx11-dev libxkbcommon-dev libwayland-dev libgtk-3-dev mesa-vulkan-drivers git-lfs
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh     # accept the default install
source "$HOME/.cargo/env"
git lfs install
```
`libgtk-3-dev` is for the native file dialogs; `mesa-vulkan-drivers` provides the software renderer (lavapipe) used for CPU-only mode.

### Editor (VS Code)
- **rust-analyzer** (`rust-lang.rust-analyzer`): completion, inline errors, go-to-definition.
- **CodeLLDB** (`vadimcn.vscode-lldb`) for debugging, or the Microsoft **C/C++** extension on Windows.

### Smoke test
```bash
cargo new hello && cd hello && cargo run      # prints "Hello, world!"
```
The first build downloads the crate index, so it takes a little longer.

### Keeping it up to date
`rustup update` updates the compiler; the project pins its dependency versions in `Cargo.lock`.
