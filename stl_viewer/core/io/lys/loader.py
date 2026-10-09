"""Turn a .lys archive into a Scene: models with their Lychee placement, supports, build plate."""

import os
from pathlib import Path

import numpy as np

from ...scene import Plate, Scene, SceneObject
from . import codec, mesh_bin, supports
from .container import Archive


def _vec(d, default=0.0) -> np.ndarray:
    d = d or {}
    return np.array([d.get("x", default), d.get("y", default), d.get("z", default)], dtype=float)


def object_matrix(obj: dict, vertices: np.ndarray) -> np.ndarray:
    """Object -> world for a mesh centred on its origin (as Lychee stores it).

    Rotation is three.js Euler XYZ in degrees about the mesh origin. `position` x/y place that origin;
    `position.z` is the height of the object's lowest point after rotation (the support lift).
    `center` is the part's centre in its original file and is not used for placement.
    """
    rx, ry, rz = np.radians(_vec(obj.get("rotation")))
    cx, sx, cy, sy, cz, sz = np.cos(rx), np.sin(rx), np.cos(ry), np.sin(ry), np.cos(rz), np.sin(rz)
    rot_x = np.array([[1, 0, 0], [0, cx, -sx], [0, sx, cx]])
    rot_y = np.array([[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]])
    rot_z = np.array([[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]])
    linear = rot_x @ rot_y @ rot_z @ np.diag(_vec(obj.get("scale"), 1.0))
    matrix = np.eye(4)
    matrix[:3, :3] = linear
    matrix[:3, 3] = _vec(obj.get("position"))
    matrix[2, 3] -= (vertices @ linear[2]).min()
    return matrix


def _object_mesh(archive: Archive, obj: dict):
    """Return (mesh, None) or (None, reason it was skipped)."""
    props = obj.get("properties") or {}
    mesh_hash = props.get("hash") or ""
    if obj.get("type") == "libraryGeometry" or mesh_hash.endswith(".lyd"):
        return None, "Lychee Library model (.lyd), not supported"
    if obj.get("type") == "libraryFile":  # geometry lives outside the archive, e.g. Lychee's bundled benchy.stl
        path = props.get("file")
        if not path or not os.path.exists(path):
            return None, f"external file not found: {path}"
        from ..registry import load_mesh

        mesh = load_mesh(path)
        mesh.vertices -= (mesh.vertices.min(0) + mesh.vertices.max(0)) / 2  # centre it like embedded meshes
        return mesh, None
    member = f"{mesh_hash}.bin"
    if member not in archive:
        return None, f"mesh {member} missing from archive"
    return mesh_bin.parse(archive.read(member)), None


def load(path, support_sides: int = supports.SIDES) -> Scene:
    path = Path(path)
    scene = Scene()
    with Archive(path) as archive:
        if "scene.bin" not in archive:
            raise codec.LysFormatError("scene.bin missing from archive")
        state = codec.decode(archive.read("scene.bin"), ciphered=not archive.is_zip)

        objects = state.get("objects", {}).get("present", {})
        placed = {}  # object id -> transform, for objects that were loaded
        models = {}  # object id -> SceneObject
        for oid in objects.get("allIds", []):
            obj = objects["byId"][oid]
            name = obj.get("name") or oid
            mesh, reason = _object_mesh(archive, obj)
            if mesh is None:
                scene.skipped.append(f"{name}: {reason}")
                continue
            placed[oid] = object_matrix(obj, mesh.vertices)
            models[oid] = SceneObject(name, mesh, placed[oid], visible=obj.get("visible", True), source=str(path))

    support_meshes = supports.build(state.get("supports", {}).get("present", {}), placed, objects.get("byId", {}), support_sides)
    for oid, model in models.items():  # each model followed by its own supports
        scene.objects.append(model)
        if oid in support_meshes:
            scene.objects.append(
                SceneObject("Supports", support_meshes[oid], kind="support", source=str(path), parent=model)
            )

    for plate in state.get("plates", {}).get("present", {}).get("byId", {}).values():
        if plate.get("visible", True) and not plate.get("isVirtual"):
            scene.plates.append(Plate(tuple(_vec(plate.get("size")).tolist()), tuple(_vec(plate.get("position")).tolist())))
    return scene
