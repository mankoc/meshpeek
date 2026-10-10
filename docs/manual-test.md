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
- [ ] F fits everything; 0–6 switch to the standard views; the gizmo's axis ends do the same when clicked.
- [ ] W toggles wireframe (View menu entry disabled if the GPU cannot draw lines).
- [ ] The plate, build-volume outline and origin axes appear for `.lys` files.
- [ ] Resizing the window keeps the image sharp and correctly proportioned.

## Objects panel
- [ ] Each `.lys` model has a *Supports* child; the tooltip shows file and triangle count.
- [ ] Unticking a model or its supports hides it.
- [ ] Right-click → *Center on object*: the view frames that object and orbits around it; standard views keep
      framing it; the 🎯 icon appears; clicking it (or F) frames everything again.
- [ ] View → Objects panel hides and shows the panel.

## Preferences (Ctrl+,)
- [ ] Changing model/support material and colour previews immediately; *Cancel* restores; *OK* keeps it after
      a restart.
- [ ] Changing *Support detail* or *OBJ files: Y is up* and pressing OK reloads the open files.
- [ ] PLY files with vertex colours keep their own colours.

## Help → About
- [ ] Shows the version, the renderer in use (GPU name, or "Microsoft Basic Render Driver" / llvmpipe with
      `--cpu`) and the memory in use.

## Installers
- [ ] Windows: the MSI installs MeshPeek under Program Files with a Start-menu entry and icon; installing a newer
      version replaces the old one; uninstall removes it.
- [ ] Linux: `sudo apt install ./meshpeek_*.deb` installs `meshpeek`, a menu entry with icon, and pulls in
      `mesa-vulkan-drivers`.
