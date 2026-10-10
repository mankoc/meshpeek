# Things to implement

## Open checks

* Space mouse: verify with the real device on Windows (no 3DxWare driver) and Linux (spacenavd and raw HID); fix
  the default axis directions if any move the wrong way (see `docs/manual-test.md`).
* Keyboard navigation: confirm the speeds feel right (orbit 90°/s, pan 60% of the view per second, zoom, roll).
* Linux packages: first real build of the `.tar.gz` and `.deb` (GitHub *Build packages* workflow or
  `scripts/build.sh` on Ubuntu).
