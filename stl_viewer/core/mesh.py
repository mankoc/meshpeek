from dataclasses import dataclass

import numpy as np

FACE_DTYPE = np.int32  # 32-bit indices: half the memory of int64, and what VTK/OpenGL use for rendering anyway


@dataclass
class Mesh:
    """Triangle mesh as plain arrays: vertices (N, 3) float32, faces (M, 3) int32 vertex indices.

    `colors` is (N, 3) uint8 per-vertex RGB when the file provides it, otherwise None.
    """

    vertices: np.ndarray
    faces: np.ndarray
    colors: np.ndarray | None = None

    @property
    def n_faces(self) -> int:
        return len(self.faces)

    @classmethod
    def from_corners(cls, corners: np.ndarray) -> "Mesh":
        """Indexed mesh from (3M, 3) triangle corner coordinates, merging corners at identical positions.

        Formats like STL repeat every shared vertex for each triangle (about 6x on a closed mesh); merging
        them cuts vertex memory and GPU vertex work by that factor.
        """
        corners = np.ascontiguousarray(corners, dtype=np.float32).reshape(-1, 3)
        if len(corners) == 0:
            return cls(corners, np.zeros((0, 3), FACE_DTYPE))
        bits = corners.view(np.uint32).astype(np.uint64)  # exact match on the float bit patterns
        key_xy = (bits[:, 0] << np.uint64(32)) | bits[:, 1]
        key_z = bits[:, 2]
        order = np.lexsort((key_z, key_xy))
        sorted_xy, sorted_z = key_xy[order], key_z[order]
        first = np.ones(len(order), bool)
        first[1:] = (sorted_xy[1:] != sorted_xy[:-1]) | (sorted_z[1:] != sorted_z[:-1])
        faces = np.empty(len(order), FACE_DTYPE)
        faces[order] = np.cumsum(first) - 1
        return cls(corners[order[first]], faces.reshape(-1, 3))

    @classmethod
    def merge(cls, meshes: list["Mesh"]) -> "Mesh":
        offsets = np.cumsum([0] + [len(m.vertices) for m in meshes[:-1]])
        return cls(
            np.concatenate([m.vertices for m in meshes]),
            np.concatenate([m.faces + FACE_DTYPE(off) for m, off in zip(meshes, offsets)]).astype(FACE_DTYPE),
        )
