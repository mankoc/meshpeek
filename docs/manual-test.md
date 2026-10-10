# MeshPeek manual test checklist

Run on Windows and on Linux before a release (`meshpeek`, then again with `meshpeek --cpu`).
Sample files are in `samples/` (Git LFS).

## Opening files
- [ ] File → Open… (Ctrl+O) opens several files at once; the status bar shows objects, triangles and time.
- [ ] Dragging files onto the window opens them; unsupported extensions are ignored.
- [ ] `meshpeek file1 file2` opens both and frames all of them.
- [ ] File → Open Recent lists the last files (newest first, numbered); a deleted file appears greyed out;
      *Clear list* empties it.
- [ ] A damaged or unsupported file shows "Could not open …" in the status bar and nothing else breaks.
- [ ] A `.lys` with Lychee Library (`.lyd`) models lists them as ⚠ skipped.

## Viewing
- [ ] Left drag orbits, middle or Shift+left drag pans, wheel and right drag zoom.
- [ ] Holding the arrow keys orbits smoothly; Shift+arrows pan; + / − and Page Up / Page Down zoom; Q / E roll.
      Typing in a text field (e.g. Preferences) does not move the view.
- [ ] View → Theme: Light, Dark and System switch the UI and the 3D background/plate colours immediately and
      are remembered after a restart.
- [ ] F fits everything; 0–6 switch to the standard views; the gizmo's axis ends do the same when clicked.
- [ ] W toggles wireframe (View menu entry disabled if the GPU cannot draw lines).
- [ ] The plate, build-volume outline and origin axes appear for `.lys` files.
- [ ] Resizing the window keeps the image sharp and correctly proportioned.

## Objects panel
- [ ] Each `.lys` model has a *Supports* child; the tooltip shows file and triangle count.
- [ ] Unticking a model or its supports hides it.
- [ ] Every model row has a grey crosshair icon (Supports rows have none). Clicking it frames that model and
      orbits around it; its icon turns blue and filled; standard views keep framing it.
- [ ] Clicking another model's icon moves the centring there (only one blue icon at a time); clicking the blue
      icon (or F) frames everything again.
- [ ] View → Objects panel hides and shows the panel.

## Preferences (Ctrl+,)
- [ ] Changing model/support material and colour previews immediately; *Cancel* restores; *OK* keeps it after
      a restart.
- [ ] Changing *Support detail* or *OBJ files: Y is up* and pressing OK reloads the open files.
- [ ] PLY files with vertex colours keep their own colours.

## Help menu
- [ ] F1 / Help → Controls lists all controls; Help → User guide opens and scrolls.
- [ ] Help → About shows the version, the renderer in use (GPU name, or "Microsoft Basic Render Driver" /
      llvmpipe with `--cpu`), the memory in use, the space mouse, and the license; *CC BY-NC-SA 4.0* opens the
      license page and *Third-party licenses* opens THIRD-PARTY-LICENSES.html (installed app / unpacked zip).

## Space mouse (3Dconnexion)
- [ ] Windows without 3DxWare: About shows the device; the cap pans, zooms, orbits and rolls the view smoothly;
      button 1 fits all, button 2 shows the isometric view.
- [ ] Linux with spacenavd running: About shows "spacenavd"; the same controls work.
- [ ] Linux without spacenavd, .deb installed: About shows the device (udev rule grants access).
- [ ] Preferences → Space mouse: sensitivity and the Pan / Zoom / Rotation inversions take effect (previewed
      while the dialog is open); unticking Enabled stops it.
- [ ] Plugging the device in while MeshPeek runs is detected within a few seconds.

## Installers
- [ ] Windows: the MSI installs MeshPeek under Program Files (with LICENSE.txt and THIRD-PARTY-LICENSES.html)
      and a Start-menu entry and icon; installing a newer version replaces the old one; uninstall removes it.
- [ ] Linux: `sudo apt install ./meshpeek_*.deb` installs `meshpeek`, a menu entry with icon, and pulls in
      `mesa-vulkan-drivers`.
