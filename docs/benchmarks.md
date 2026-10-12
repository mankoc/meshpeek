# MeshPeek benchmarks

Machine: Windows 11, NVIDIA GeForce RTX 3070. Rust numbers from `meshpeek-bench` / `meshpeek-info`
(release build); Python numbers from the earlier Python version (branch `python`).
"Process memory" is the whole process, including the GPU driver's share.

## Sample scene: bust `.lys` + pilone OBJ + benchy STL (3.27M triangles, 1600×1000)

| Metric | Python (VTK) | MeshPeek, GPU | MeshPeek, CPU only (WARP) |
|---|---|---|---|
| Load all three files | 1.7 s | **0.13 s** | 0.10 s |
| Frame time while orbiting (median) | 3.8 ms | **0.9 ms** | 77 ms |
| Process memory | 1115 MB | **248 MB** | 216 MB |

## Per-file load time (parse only)

| File | Python | MeshPeek |
|---|---|---|
| Bust `.lys` (2M triangles, supports generated) | 51 ms | **30 ms** |
| Pilone OBJ (85 MB, 1.06M triangles) | 1688 ms | **40 ms** |
| Benchy STL (225k triangles, vertex merging) | 79 ms | **34 ms** |

## Very large model: synthetic 30M-triangle binary STL (1.5 GB file)

| Metric | MeshPeek, GPU | MeshPeek, CPU only (WARP) |
|---|---|---|
| Load (parse + merge vertices) | 4.5 s | 4.3 s |
| Upload to GPU | 0.23 s | 0.23 s |
| Frame time (median) | **5.9 ms** (~170 fps) | 468 ms (~2 fps) |
| GPU buffers | 515 MB | 515 MB (in system RAM) |
| Process memory after upload | **216 MB** (CPU copy freed) | 658 MB |

Notes:
- MSAA 4× on GPUs; off on software renderers (it multiplies their per-pixel cost: 179 → 77 ms on the
  sample scene).
- CPU-only mode on huge models is usable but slow. A reduced-detail model while orbiting would help; it is a
  candidate follow-up (see the plan).

Reproduce:
```
cargo build --release -p meshpeek-tools
target/release/meshpeek-bench [--cpu] "samples/Favorite_BUST - Lychee.lys" samples/Pilone_DX.obj <benchy.stl>
target/release/meshpeek-gen target/torus30m.stl 30000000 && target/release/meshpeek-bench target/torus30m.stl
```
