"""Wavefront OBJ reader: geometry only (`v` and `f`), vectorised with numpy.

Everything else (materials, normals, texture coordinates, lines, groups) is ignored, so files that
trip stricter readers, e.g. Blender's `usemtl` with an empty name, still load.
"""

import numpy as np

from ..mesh import Mesh

_SPACE, _TAB, _CR, _LF, _MINUS, _ZERO, _NINE = 32, 9, 13, 10, 45, 48, 57


def _parse_faces(f_lines: list[bytes]) -> tuple[np.ndarray, np.ndarray]:
    """(vertex index of every corner, corner count of every face), straight from the bytes.

    Corners look like v, v/vt, v//vn or v/vt/vn; only the leading (possibly negative) integer is read.
    """
    data = np.frombuffer(b"\n".join(f_lines) + b"\n", np.uint8)
    newline = data == _LF
    blank = newline | (data == _SPACE) | (data == _TAB) | (data == _CR)
    starts = np.flatnonzero(~blank & np.concatenate([[True], blank[:-1]]))
    sizes = np.bincount(np.cumsum(newline)[starts], minlength=len(f_lines))

    negative = data[starts] == _MINUS
    starts = starts + negative
    is_digit = (data >= _ZERO) & (data <= _NINE)
    non_digit = np.flatnonzero(~is_digit)
    ends = non_digit[np.searchsorted(non_digit, starts)]  # first byte after the index digits
    lengths = ends - starts
    if lengths.min() < 1:
        raise ValueError("face corner without a vertex index")
    values = np.zeros(len(starts), np.int64)
    for k in range(int(lengths.max())):  # positional decimal parse, one digit column at a time
        has_digit = k < lengths
        values[has_digit] = values[has_digit] * 10 + (data[starts[has_digit] + k] - _ZERO)
    return np.where(negative, -values, values), sizes


def load(path) -> Mesh:
    with open(path, "rb") as f:
        lines = f.read().splitlines()
    v_lines = [line[2:] for line in lines if line.startswith((b"v ", b"v\t"))]
    f_lines = [line[2:] for line in lines if line.startswith((b"f ", b"f\t"))]
    del lines
    if not v_lines or not f_lines:
        raise ValueError(f"{path}: no triangles found")

    values = np.array(b" ".join(v_lines).split(), dtype=np.float64)  # several times faster than np.fromstring
    per_vertex = len(v_lines[0].split())
    if len(values) == per_vertex * len(v_lines):  # uniform layout: x y z [w] or x y z r g b
        table = values.reshape(len(v_lines), per_vertex)
    else:  # mixed layouts: fall back to parsing each line
        table = np.array([[float(x) for x in line.split()[:3]] for line in v_lines])
    vertices = np.ascontiguousarray(table[:, :3], dtype=np.float32)
    colors = None
    if table.shape[1] == 6:  # vertex colours as 0..1 floats
        colors = np.clip(table[:, 3:6] * 255 + 0.5, 0, 255).astype(np.uint8)

    try:
        corners, sizes = _parse_faces(f_lines)
    except ValueError as e:
        raise ValueError(f"{path}: {e}") from None
    n_vertices = len(vertices)
    corners = np.where(corners < 0, corners + n_vertices, corners - 1)  # 1-based; negative = from the end
    starts = np.cumsum(sizes) - sizes
    triangles = []
    for size in np.unique(sizes[sizes >= 3]):  # fan-triangulate each polygon size in one go
        polys = corners[starts[sizes == size][:, None] + np.arange(size)]
        triangles += [polys[:, [0, k, k + 1]] for k in range(1, size - 1)]
    if not triangles:
        raise ValueError(f"{path}: no triangles found")
    faces = np.concatenate(triangles)
    if faces.min() < 0 or faces.max() >= n_vertices:
        raise ValueError(f"{path}: face refers to a vertex that does not exist")
    return Mesh(vertices, faces, colors)
