"""Embedded object meshes: `<hash>.bin` and `<hash>_hollowing.bin`.

Layout (verified on sample files): five uint32 (version=2, header_len, n_indices, n_coords, reserved),
then n_indices uint32 and n_coords float32. Data always starts at byte 20 even though header_len reads 12.
"""

import struct

import numpy as np

from ...mesh import FACE_DTYPE, Mesh
from .codec import LysFormatError

HEADER_SIZE = 20


def parse(raw: bytes) -> Mesh:
    if len(raw) < HEADER_SIZE:
        raise LysFormatError("mesh member too short")
    version, _, n_indices, n_coords, _ = struct.unpack_from("<5I", raw)
    if version != 2:
        raise LysFormatError(f"unsupported mesh version {version}")
    if len(raw) != HEADER_SIZE + 4 * (n_indices + n_coords) or n_indices % 3 or n_coords % 3:
        raise LysFormatError("mesh member size does not match its header")
    faces = np.frombuffer(raw, "<u4", n_indices, HEADER_SIZE).reshape(-1, 3).astype(FACE_DTYPE)
    vertices = np.frombuffer(raw, "<f4", n_coords, HEADER_SIZE + 4 * n_indices).reshape(-1, 3).copy()
    return Mesh(vertices, faces)
