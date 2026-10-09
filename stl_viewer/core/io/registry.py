"""Entry point for loading any supported file into a Scene."""

from dataclasses import dataclass
from pathlib import Path

import numpy as np

from ..mesh import Mesh
from ..scene import Scene, SceneObject
from . import obj, stl, vtk_formats

MESH_LOADERS = {".stl": stl.load, ".obj": obj.load, ".ply": vtk_formats.load}
SUPPORTED_EXTENSIONS = (*MESH_LOADERS, ".lys")

# Rotation +90 degrees about X: a Y-up model (x, y, z) becomes Z-up (x, -z, y).
Y_UP_TO_Z_UP = np.array([[1.0, 0, 0, 0], [0, 0, -1, 0], [0, 1, 0, 0], [0, 0, 0, 1]])


@dataclass(frozen=True)
class LoadOptions:
    support_sides: int = 12  # facets around .lys support columns and their joint spheres
    obj_y_up: bool = True  # OBJ files use Y as up (Blender, Maya, ZBrush); rotate them to this viewer's Z-up


def load_mesh(path: str | Path) -> Mesh:
    """Load a single-mesh file (STL, OBJ, PLY)."""
    ext = Path(path).suffix.lower()
    if ext not in MESH_LOADERS:
        raise ValueError(f"Unsupported mesh format: {ext}")
    return MESH_LOADERS[ext](path)


def load(path: str | Path, options: LoadOptions = LoadOptions()) -> Scene:
    """Load any supported file. Single meshes keep their file coordinates, apart from the OBJ up-axis fix."""
    path = Path(path)
    ext = path.suffix.lower()
    if ext == ".lys":
        from .lys import load as load_lys

        return load_lys(path, options.support_sides)
    transform = Y_UP_TO_Z_UP.copy() if ext == ".obj" and options.obj_y_up else np.eye(4)
    return Scene(objects=[SceneObject(path.stem, load_mesh(path), transform, source=str(path))])
