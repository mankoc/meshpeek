from dataclasses import dataclass

import numpy as np


@dataclass
class Mesh:
    """Triangle mesh as plain arrays: vertices (N, 3) float32, faces (M, 3) int64 vertex indices.

    `colors` is (N, 3) uint8 per-vertex RGB when the file provides it, otherwise None.
    """

    vertices: np.ndarray
    faces: np.ndarray
    colors: np.ndarray | None = None

    @property
    def n_faces(self) -> int:
        return len(self.faces)

    @classmethod
    def triangle_soup(cls, corners: np.ndarray) -> "Mesh":
        """Mesh from (3M, 3) corner coordinates, three per triangle, without merging shared vertices."""
        vertices = np.ascontiguousarray(corners, dtype=np.float32).reshape(-1, 3)
        return cls(vertices, np.arange(len(vertices), dtype=np.int64).reshape(-1, 3))

    @classmethod
    def merge(cls, meshes: list["Mesh"]) -> "Mesh":
        offsets = np.cumsum([0] + [len(m.vertices) for m in meshes[:-1]])
        return cls(
            np.concatenate([m.vertices for m in meshes]),
            np.concatenate([m.faces + off for m, off in zip(meshes, offsets)]),
        )
