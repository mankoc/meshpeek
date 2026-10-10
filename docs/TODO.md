# Things to implement

* Add hotkeys for moving and rotating the view
* Add support for space mouse
* add help to the help menu
* add creative commons non commercial license
* add a switch to change between light-dark modes
* change the right-click, center on model to an icon like it appears on when centered that can be clicked to make center on object or disable it, only one should be selected, add icons for selected/unselected

---

## Plan

**Status (2026-10-10): all six items implemented.** Items 1–5 verified in the app (snapshots) and by tests; the
space mouse (item 6) has unit-tested report decoding and needs a check with the real device on Windows and
Linux (see `docs/manual-test.md`).

Decisions (2026-10-10): license **CC BY-NC-SA 4.0**; space mouse on **Windows without driver** (raw HID) and
**Linux** (spacenavd); centre icon on **models only**. Implementation order is smallest first.

### 1. Centre-on-object icon per row (replaces the right-click menu)

* Every model row gets an icon at its right edge, drawn with the egui painter (no emoji):
  * not centred: grey outline crosshair; hover highlights it. Click → centre on the model (and orbit around it).
  * centred: filled accent-coloured crosshair. Click → stop centring and fit everything (same as F).
* Only one can be selected: the viewport keeps a single `focus: Option<ObjectId>`.
* Supports rows get no icon. Remove the right-click *Center on object* menu.
* Files: `crates/meshpeek-app/src/app.rs` (`object_row`, new `focus_icon` helper).
* Done when: clicking icons switches the centred model; tooltips explain both states; manual-test list updated.

### 2. Light / dark mode switch

* *View → Theme: System / Light / Dark*, stored in `settings.toml` (`theme = "system" | "light" | "dark"`),
  applied with `ctx.set_theme(...)`.
* 3D view follows the theme: background gradient, plate and outline colours become renderer inputs
  (`Renderer::set_palette`) instead of constants in `shaders/flat.wgsl`. Model/support colours unchanged.
* Files: `settings.rs`, `app.rs`, `meshpeek-render/src/renderer.rs`, `shaders/flat.wgsl`.
* Done when: switching is instant, persists across restarts, "System" follows the OS setting; render test covers
  both palettes.

### 3. Help menu

* *Help → Controls* (F1): window with all mouse, keyboard and space-mouse controls, generated from one shortcut
  table in the code (also used by the menus).
* *Help → User guide*: in-app guide window (opening files, navigation, objects panel, preferences, `.lys`
  notes, CPU mode) so it works offline.
* *Help → About*: add the license line and a link/button to the third-party licenses file.
* Files: new `crates/meshpeek-app/src/help.rs`, `app.rs`.

### 4. Keyboard navigation

| Keys | Action |
|---|---|
| ← → ↑ ↓ | Orbit, continuous while held (~90°/s) |
| Shift + arrows | Pan |
| + / − or Page Up / Page Down | Zoom |
| Q / E | Roll around the view direction |

* Frame-rate independent (scaled by frame time); repaint only while a key is held; ignored while typing.
* New `Camera::roll` (+ unit test). Files: `meshpeek-render/src/camera.rs`, `meshpeek-app/src/viewport.rs`.
* Listed in Help → Controls and the README.

### 5. License: CC BY-NC-SA 4.0

* `LICENSE` with the official legal code; `license = "CC-BY-NC-SA-4.0"` in `Cargo.toml`; README section and
  About line.
* Third-party notices: generate `THIRD-PARTY-LICENSES.html` with `cargo-about` in the build scripts and ship it
  in the zip, tar.gz, MSI and .deb (required by the MIT/Apache licenses of the ~300 bundled crates).
* Note: Creative Commons does not recommend its licenses for software (no source-code or patent terms); accepted
  knowingly for the non-commercial restriction.

### 6. Space mouse (3Dconnexion)

* Input thread produces 6-DOF motion + button events over a channel; the viewport applies them each frame:
  translation X/Y → pan, Z → zoom, rotations → orbit/roll; button 1 → Fit all, button 2 → Isometric.
* Backends:
  * **Windows / Linux, raw HID** via `hidapi` (3Dconnexion vendor ids 0x046d / 0x256f; report 1 = translation,
    2 = rotation, 3 = buttons, plus the newer combined report). Works on Windows without 3DxWare.
  * **Linux, spacenavd** via its Unix socket (`/var/run/spnav.sock`), used first when the daemon runs (the
    device is then busy for raw HID). Raw HID on Linux needs a udev rule — shipped in the .deb.
* Preferences: enable, sensitivity, invert per axis, dominant-axis mode. Status/About show the detected device.
* Files: new `crates/meshpeek-app/src/spacemouse.rs` (+ `hidapi` dependency), `viewport.rs`, `settings.rs`,
  `assets/99-meshpeek-spacemouse.rules` for the .deb.
* Testing: unit tests decode recorded HID reports; manual test on your Windows and Linux machines (no device
  available here).

### Order and size (effort)

1. Centre icon — small
2. Light/dark — small/medium
3. Help menu — small
4. Keyboard navigation — small
5. License + third-party notices — small
6. Space mouse — medium/large (hardware-dependent testing)
