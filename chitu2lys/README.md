# chitu2lys

Converts a CHITUBOX project (`.chitubox`) into a Lychee Slicer scene (`.lys`).

```text
chitu2lys <project.chitubox> [scene.lys] [--template settings.lys]
```

Without an output name, the scene is written next to the project with the `.lys` extension.

### Window

`chitu2lys-gui` does the same in a small window: drop `.chitubox` files on it (or *Add projects…*), choose where
the scenes go (next to each project, or a folder) and optionally a `.lys` to take the printer from, and press
**Convert**. Each project shows how it went: the number of models and supports, or why it failed. Projects can also
be passed on the command line (`chitu2lys-gui a.chitubox b.chitubox`).

## What is converted

CHITUBOX supports are trees: tips lead through columns to a node where the tips of a tree meet, and from there a
trunk runs down to a foot pad, to the model (a bottom tip), or onto another tree's trunk; crossing braces join the
trunks. chitu2lys follows every tip down its columns and rebuilds the trees with Lychee's own support types along
the same paths, so Lychee can edit them:

| CHITUBOX | Lychee |
|---|---|
| Each model, at its position on the plate and its lift | An imported model at the same position and height |
| A tree standing on a foot pad | A plate support on the same foot, its column rising to where the tips meet, carrying one of the tips |
| A tree standing on the model (bottom tip) | A support standing on the model at the same point, its base cone the bottom tip |
| A tree leaning on another tree's trunk | A support resting on that trunk's support at the same point |
| Every other tip of the tree | A branch resting on that support where the tips meet, following CHITUBOX's branch and its bend |
| Tip contact point, direction, length, contact and column diameters, contact depth | The same on every Lychee support |
| Trunk and branch diameters | The same column and branch diameters |
| Each brace between two trunks | A Lychee brace between the same supports, at the same heights and diameter |
| The square foot pads (measured from their geometry) | Square Lychee foot pads of the same size, thickness and slope |
| The project preview | The scene preview |

A Lychee support draws at most two straight pieces below its tip cone (a column or base cone, and a middle). Each
support takes the joins, among the bends of its CHITUBOX path, that keep its drawing closest to CHITUBOX's; where
a path has one bend too many (mostly trees on the model whose top bends sideways), the tip cone is stretched over
the last piece or the middle runs straight past the bend, whichever is closer. The tool prints how much of
CHITUBOX's support paths is drawn exactly (typically 98–99.9 %); the rest stays within about a millimetre.

The few tips whose path cannot be followed (they end at a joint without a column below) stand straight below their
tip, on the plate or on the model. Lychee draws its own shapes for tips and pads and no joint balls. Models keep
CHITUBOX's hollowing; Lychee's own hollowing and raft are left switched off.

## Printer and settings

A `.lys` file also holds the printer, resins and Lychee settings. They come from the built-in template
(`template/scene.json`, saved by Lychee 8), or from `--template` with any `.lys` saved with the printer you want.
The tool notes models that stick out of the template printer's plate; choose a larger printer in Lychee if needed.

## How it works

- Reads the project with MeshPeek's CHITUBOX reader (`meshpeek-core`, format in
  `docs/formats/CHITUBOX_FILE_FORMAT.md`).
- Writes the Lychee container, scene state and meshes as described in `docs/formats/LYS_FILE_FORMAT.md`
  (`src/lys.rs`), starting from the records in `template/` (`src/convert.rs`).
- Rebuilds the support trees (`src/trees.rs`), and finds where a loose tip's support lands with a vertical ray
  cast against the model (`src/ground.rs`).
