# Changelog

All notable changes to MeshPeek, newest first.

## 0.3.0 — 2026-10-12

### Changed
- **Rafts** are now a port of Lychee's own code. `.lys` objects whose `foundation` is enabled show their raft; all
  eight types are supported (`cylinder`, `cube`, `rectangle`, `grid`, `triangle`, `vectoriel`, `vectorielWall`,
  `pixelate` / `pixelateRound`). A raft follows the object's footprint on the plate (the `triangle` type: thin bars
  between cylinders under the supports); the `angle` is a slope in percent (the top is `size * angle / 100` wider
  than the bottom); `vectorielWall` is a dish with a rim wall that leans outwards. The silhouette types use the
  convex hull or a raster of the object's projection, an approximation of Lychee's traced contour.
- **Support geometry** is now a port of Lychee's own code: every part is a capsule (two spheres joined by a cone,
  rounded ends), the join positions, radii and flat ends follow Lychee's rules, mini supports and braces use its
  short cones (a third of their length, at most 2 mm) and thin radii, braces match the diameter of the support they
  rest on, the foot pad has Lychee's shape and the join cone sits on it. Supports on a raft lose their foot pad and
  start with their join cone on top of the raft.
- **All support shape types**: `cube`, `cylinder` and `cross` tips and base tips, `cube` / `cross` middles and columns,
  `cube`, `cube45`, `octagon`, `cone` and `cross` foot pads, and break points.
- Silhouette rafts with `baseSupport` now cover the feet of the plate supports (Lychee paints a disc around each foot
  into the outline), so a support standing outside the model's outline no longer pokes out of the raft.
- **Braces** between two supports are straight again: Lychee draws each as one capsule from its base to the other
  support, narrowing from the diameter of the support it starts on to that of the support it ends on, and it
  recomputes the normals along that line. I had used the stored normals (those of the parent supports), which gave
  bent braces.

## 0.2.3 — 2026-10-12

### Added
- **Raft:** `.lys` scenes that have the raft switched on now show it: a flat disc under every foot, joined by bars
  to the neighbouring feet. Lychee does not store the raft's shape, so this is an approximation.
- A manual **Build exe** GitHub workflow that builds only the Windows executable.
- Releases now also include the **bare `meshpeek.exe`** (no archive, no installer), next to the zip and the MSI.

### Changed
- **Mouse orbit** is now a turntable by default: dragging sideways turns the model around the vertical (Z) axis
  instead of the current view axis, dragging up/down tilts, the horizon stays level and the view cannot flip over
  the poles. *Preferences → Mouse orbit* switches back to the previous trackball orbit.

### Fixed
- **Mini supports** (short bridges, also those that start on another support) are drawn as the thin uniform rods
  Lychee shows, as thick as their own `extra` point diameters, instead of bent, blunt-ended full-size tubes.
- **Supports that start on the model** (a branch with no parent support) now end in a thin base cone widening to
  the column, like Lychee, instead of a flat end with the full column diameter.
- **Rotated `.lys` objects:** an object rotated about several axes was turned in the wrong order, and the support
  feet ignored the object's yaw, so supports hung sideways and in the wrong place. Objects and their support grids
  now match Lychee (checked on a file rotated about all three axes: every branch and brace support lands on its
  parent).
- **Support rendering:** in some `.lys` files a plate support's `baseNormal` is stored in the object's own rotated
  space instead of world space, which drew long horizontal beams across the model. Both readings are now tried and
  the upward one is used, so every branch support lands on its parent column.

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
