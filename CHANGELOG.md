# Changelog

All notable changes to MeshPeek, newest first.

## 0.2.2 — 2026-10-10

### Changed
- **Keyboard:** Ctrl + arrow keys now orbit the model (Shift + arrows still pan). The plain Left / Right arrows
  open the previous / next model file of the same folder, alone in the scene (wrapping around at the ends).
- **Archives behave like folders.** Opening a `.zip`, `.7z` or `.rar` opens only its first model and lists all
  the models of the archive in the Files tab, as if it were a folder. Double-click one, or tick several and press
  *Open selected*, to open others; Left / Right step through the archive's models.
- Only the entry you open is read into memory, not the whole archive.
- *Open folder* and dropping a folder load the model files in it; archives inside it are listed in the Files tab
  (📦) instead of being unpacked automatically.
- The Files tab follows the folder (or archive) of the last file you opened.

### Fixed
- Opening a `.7z` no longer fails with a checksum error on solid archives.

## 0.2.1 — 2026-10-10

### Added
- **Archives:** open or drag and drop `.zip`, `.7z` and `.rar` files; the models inside are read straight into
  memory, nothing is written to disk.
- **Folders and Files tab:** *File → Open folder…*, a *Folder* button and folder drag and drop open every model in
  a folder (sub-folders included). A new *Files* tab next to *Objects* shows a folder tree where you can tick
  several files and open them together.
- **New application icon** (a faceted gem) and a large version of it in the About window, with the author credit.
- **Coloured toolbar icons** with the label underneath; Preferences can switch the text off.
- **Hide / show all objects:** toolbar button, *View* menu entry and the **H** key.
- **Windows installer:** MeshPeek now appears in *Open with* for STL, OBJ, PLY and LYS files (the default program
  is not changed) and adds a *Send to* entry. Preferences has *Make default app…*, which opens the Windows
  *Default apps* settings page.
- **GitHub Actions:** a *Release* workflow builds the Windows and Linux packages and publishes a GitHub release when
  a `v<version>` tag is pushed.
- The window title shows the version.

### Fixed
- Loading many models at once (for example from an archive) crashed with a stack overflow in debug builds;
  loader and parser threads now have a larger stack.

### Changed
- The *CI* workflow no longer runs on every push; start it manually from the Actions tab.

## 0.2.0 — 2026-10-10

### Added
- **Show / hide the button bar** (*View → Button bar*) and **the objects panel** (toolbar button and *View* menu).
- **Model information:** an ℹ icon next to each model opens a window with its full path, triangle count (and
  support triangles), width / depth / height, file format and size, and the STL header comment.
- **Colour each model:** a toolbar button and *View* entry give every model its own colour.
- **Hide the build plate** of `.lys` files (*Preferences → Show build plate*).
- **Esc** closes the program.

### Changed
- *Preferences…* moved from the *Settings* menu (removed) to *File*.
- The theme (System / Light / Dark) moved from *View → Theme* into *Preferences*.

## 0.1.0 — 2026-10-10

First release of the Rust version of MeshPeek (the Python viewer lives on the `python` branch).

- Fast viewer for **STL, OBJ, PLY and Lychee `.lys`** files, with a GPU renderer (wgpu) and an automatic software
  fallback when no GPU is available (`--cpu` forces it).
- `.lys` files load with their models, supports and build plate; large models stay responsive.
- Objects panel with a visibility checkbox per object (a model's supports are shown under it and follow it) and a
  crosshair icon to centre the view on a model and orbit around it.
- Orbit, pan, zoom and roll with the mouse or keyboard; standard views (**0–6**), **F** to fit all, **W** for
  wireframe, and an axis gizmo in the corner.
- **3Dconnexion space mouse** support without the vendor SDK (HID, and `spacenavd` on Linux).
- Preferences: materials and colours for models and supports, support detail, OBJ up axis, space mouse; light and
  dark theme; recent files; Help menu with Controls (F1) and an offline user guide.
- Packages: Windows zip and MSI installer, Linux `.tar.gz` and `.deb`, each with the third-party licenses.
- Licensed under CC BY-NC-SA 4.0.
