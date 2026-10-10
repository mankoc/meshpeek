# Things to implement
1- add the meshpeek version to window title

## New functionality
9- Loading models from archive files:  It should be possible to drag and drop or open a 7z, zip or rar archive and the program should open all the files in the package uncompressing the file into a memory disk.
10 - Reimplementation of how files are opened:
* we should be able to select multiple files at the same time, consider using a file tree on the left in a tab in parallel to the objects
* We should be able to load a full folder with objects at the same time
## Not code related
3 - Suggest new icons for filetype and application
4 - Add send to meshpeek option on windows on installer
11- Installer must register meshpeek as a program that opens stl, obj, lys (do not change the default association, just make sure that the app appears on the list of open with.  Add an option on configuration to make it the default for the different filetypes)

## Open checks

* Linux packages: first real build of the `.tar.gz` and `.deb` (GitHub *Build packages* workflow or
  `scripts/build.sh` on Ubuntu).
