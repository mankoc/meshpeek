"""STL reader. Binary files are read in one numpy call; ASCII files go through VTK."""

import os

import numpy as np

from ..mesh import Mesh
from . import vtk_formats

_HEADER = 84  # 80-byte comment + uint32 triangle count
_RECORD = np.dtype([("normal", "<f4", 3), ("corners", "<f4", (3, 3)), ("attr", "<u2")])


def load(path) -> Mesh:
    with open(path, "rb") as f:
        head = f.read(_HEADER)
    if len(head) == _HEADER:
        n = int.from_bytes(head[80:84], "little")
        # Size check rather than the "solid" prefix: many binary STLs start with "solid" too.
        if _HEADER + n * _RECORD.itemsize == os.path.getsize(path):
            records = np.fromfile(path, dtype=_RECORD, count=n, offset=_HEADER)
            return Mesh.from_corners(records["corners"])
    return vtk_formats.load(path)
