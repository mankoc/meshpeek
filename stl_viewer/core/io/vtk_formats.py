"""OBJ, PLY and ASCII STL via VTK's C++ readers (no Qt involved)."""

import numpy as np

from ..mesh import Mesh


def load(path) -> Mesh:
    import pyvista as pv

    data = pv.read(path)
    if not isinstance(data, pv.PolyData):
        data = data.extract_surface()
    faces = data.triangulate().regular_faces
    if len(faces) == 0:
        raise ValueError(f"{path}: no triangles found")
    colors = None
    for name in ("RGB", "RGBA"):  # names VTK's PLY reader gives to red/green/blue(/alpha) vertex properties
        if name in data.point_data:
            colors = np.ascontiguousarray(data.point_data[name][:, :3], dtype=np.uint8)
            break
    return Mesh(np.asarray(data.points, dtype=np.float32), faces.astype(np.int64), colors)
