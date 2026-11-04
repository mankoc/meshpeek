# RuneBrace `.ags` projects

Worked out from four sample files (`Version` 1.8) and a RuneBrace screenshot of one of them. The files do not name the
program. They are JSON with .NET conventions (`System.Numerics` matrix names `M11`…`M44`, GUID ids), and some field names
are Italian (`Sede`, `RaggioSfiato`).

## Layout

One JSON object with about 115 keys, most of them the program's own settings (window, camera, brushes, bracing).
The ones MeshPeek reads:

| Key | Meaning |
|---|---|
| `ModifiedMeshDataBase64Gz` | The model: Base64 of gzip of the block below |
| `ModelPath` | The original model file. MeshPeek loads it, next to the project, if the key above is empty (not seen in a sample) |
| `ModelTransform` | Model to world, `M11`…`M44`. Row vectors: `world = vertex × M` |
| `Supports` | The supports, already in world coordinates |
| `RaftThickness`, `GridSize` | Raft height and the grid the feet stand on (also when `EnableSnapToGrid` is false) |
| `UseTipSphere`, `TipSphereMultiplier` | Sphere on each tip: tip radius × multiplier |
| `ReinforcementEnabled`, `ReinforcementBaseRadiusPercent`, `ReinforcementConeHeightMM` | Radius of the foot of a support, in percent, and the height of its cone |
| `IsHollowed`, `HollowThickness`, `Holes` | Shown in the information window only |

The mesh block, all little-endian:

```text
i32  vertex count V
V ×  3 × f32   positions
i32  index count I
I ×  i32       triangle corners
i32  normal count (= V)
V ×  3 × f32   unit normals (not read)
```

## World

Y is up, the plate is at Y = 0, units are millimetres. MeshPeek turns everything to Z-up: (x, y, z) becomes
(x, -z, y). Checked on the samples: with `ModelTransform` the support tips are about 0.1 mm from the nearest vertex
(several millimetres to 25 mm without it), the matrices are rotations (determinant 1), and the meshes have outward
winding.

`Holes[].Position` is in the model's own frame, not the world.

## Supports

Each record has `ControlPoints` (X, Y, Z) from the tip to the foot and `Radii`, one per point. Fields that matter:

- `IsColumn`: a two-point pillar inside a hollow model. No tip sphere, no foot.
- `IsBranchingSupport`, `IsGroundReinforcement`: branches off a parent (`ParentSupportId`). Their control points already
  end on the parent, so they are drawn like the others.
- `UseTIpSphere`: spelled like this in the files.

A normal support has four points: the tip, a bend, a point on the column and the foot, which ends at Y = 0.25, in the
middle of the 0.5 mm raft.

MeshPeek draws a capsule between each two points. The RuneBrace screenshot shows sharp bends and round rods, so the
control points are not smoothed into curves, and fluting (`SupportFlutingEnabled`) does not show in its view.

## Not in the file, or not drawn

- **The raft** (`RaftType` `Networked` in all samples) is not stored. `SupportToGridCellMapping` maps supports to grid
  cells: a cell (X, Y) is the foot at (X × `GridSize`, Z = Y × `GridSize`). RuneBrace's raft settings show *Island Margin*
  1.00 mm, *Bridge Thickness* 0.50 mm and *Max Bridge Length* 10.0 mm for the Bust_Torso sample; none of the three is in
  the file, so MeshPeek uses those values for every project. A pad is the foot's radius plus the margin (0.825 + 1 mm:
  the 3.6 mm measured on the screenshot), and bridges are the edges of a Delaunay triangulation of the feet up to the
  longest length, 0.5 mm wide. How RuneBrace picks its bridges is not known, so they can differ one by one.
- **The bracing** (the yellow Xs in RuneBrace) is not stored either, only its settings: `Bracing_MinBracingLength`,
  `Bracing_MaxBracingLength`, `Bracing_OptimalAngleDegrees`, `Bracing_BracingRadiusFactor`, `Bracing_MinHeight`,
  `Bracing_MaxHeight`, `Bracing_VerticalStepPercent`, `Bracing_VerticalSeparationMM`, `Bracing_UseCrossedBracing` and
  `Bracing_ClearanceForCollisionCheck`. *Max Conn* (5) is in RuneBrace's panel but not in the file, and `ShowBracings` is
  only its display switch. MeshPeek draws Xs between supports that stand on the plate, nearest first, at most 5 per
  support, where a brace at the wanted angle fits between the shortest and longest length. They attach to the rod at
  each height (a rod usually rises straight for half its height, then leans towards its tip) and are stacked with the
  separation between them, from `MinHeight` to `MaxHeight` of the shorter rod, tip excluded. They are left out where they
  would come closer to the model's underside than the clearance. These readings of the settings come from screenshots,
  and `Bracing_VerticalStepPercent` is not used, so the braces match in pattern, not one by one.
- **Hollowing** is not in the stored mesh: in all samples, hollowed ones included, it is a single closed shell without an
  inner cavity. `VoxelMaskBase64` is a bit mask of `VoxelGridDimX` × `Y` × `Z` cells (all set in the samples).
- **Holes**: MeshPeek only counts them. Whether they are cut into the stored mesh is not settled: the vertices around
  their positions fit neither answer clearly, and with no cavity they could only be blind recesses.
- **Infill lattice** (`HasInfillLattice`, spacing and diameter) is generated by the program and not stored.
- **Scale settings**: `RelativeMultipliers` and `*ScalePercent` are 1 and 100 in all samples, so their effect is unknown
  and they are ignored.
- There is no printer or plate size, so the plate RuneBrace shows is not in the file.
