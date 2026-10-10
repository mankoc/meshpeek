# Things to implement

1- Loading models from archive files:  It should be possible to drag and drop or open a 7z, zip or rar archive and the program should open all the files in the package uncompressing the file into a memory disk.
2- For lys files the plate should be able to be hidden on configuration
3- Installer must register meshpeek as a program that opens stl, obj, lys (do not change the default association, just make sure that the app appears on the list of open with.  Add an option on configuration to make it the default for the different filetypes)
4- open button should open a file selector that let´s us select multiple files from a list (suggest options for this)
## Open checks

* Linux packages: first real build of the `.tar.gz` and `.deb` (GitHub *Build packages* workflow or
  `scripts/build.sh` on Ubuntu).
