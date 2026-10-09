"""Cast shadows on the models: a shadow map rendered once from the sun, tested per vertex with numpy.

The sun is fixed in the scene, so the map only changes when the scene does. The result is a lit factor
per vertex (0 = in shadow, 1 = lit) that the viewport turns into colours, so drawing a frame costs nothing
extra. The map is rendered in its own square off-screen window: VTK's built-in shadow pass assumes a
square main window and misplaces shadows otherwise.
"""

import numpy as np
import pyvista as pv
from vtkmodules.util.numpy_support import vtk_to_numpy
from vtkmodules.vtkCommonCore import vtkFloatArray
from vtkmodules.vtkRenderingCore import vtkPolyDataMapper

RESOLUTION = 4096  # shadow map size; ~0.04 mm per texel on a 150 mm plate
BIAS_TEXELS = 1.5  # depth tolerance against self-shadowing "acne" on lit surfaces
NORMAL_OFFSET_TEXELS = 1.5  # look up from slightly outside the surface: removes acne at grazing angles
PCF_RADIUS = 1  # samples (2r+1)^2 texels around each vertex for softer edges
TERMINATOR = 0.15  # surfaces facing the sun less than this (cosine) fade into shadow


def bake(
    objects: list[tuple[pv.PolyData, np.ndarray]], light_direction: np.ndarray, normal_cache: dict | None = None
) -> list[np.ndarray]:
    """Lit factor per vertex for each (polydata, object->world matrix); hidden objects must be left out.

    `light_direction` points from the scene toward the sun. `normal_cache` (keyed by the mesh's VTK address) keeps
    the vertex normals between bakes, since meshes never change after loading.
    """
    if not objects:
        return []
    cache = {} if normal_cache is None else normal_cache
    light = np.asarray(light_direction, float) / np.linalg.norm(light_direction)
    world = [_to_world(poly, matrix) for poly, matrix in objects]
    normals = []
    for poly, matrix in objects:
        key = poly.GetAddressAsString("vtkPolyData")  # stable per mesh; Python wrappers (and id()) are not
        if key not in cache:
            cache[key] = _vertex_normals(poly)
        normals.append(_to_world_normals(cache[key], matrix))
    points = np.concatenate(world)
    centre = (points.min(0) + points.max(0)) / 2
    radius = max(np.linalg.norm(points.max(0) - points.min(0)) / 2, 1e-3)

    plotter = pv.Plotter(off_screen=True, window_size=(RESOLUTION, RESOLUTION), lighting="none")
    try:
        for poly, matrix in objects:
            mapper = vtkPolyDataMapper()
            mapper.SetInputData(poly)
            mapper.SetScalarVisibility(False)
            actor = pv.Actor(mapper=mapper)
            actor.user_matrix = matrix
            plotter.add_actor(actor, reset_camera=False)
        camera = plotter.camera
        camera.parallel_projection = True
        camera.focal_point = tuple(centre)
        camera.position = tuple(centre + light * 3 * radius)
        up = np.cross(light, [1.0, 0, 0] if abs(light[0]) < 0.9 else [0, 1.0, 0])
        camera.up = tuple(up / np.linalg.norm(up))
        camera.parallel_scale = radius * 1.01
        camera.clipping_range = (radius, 5 * radius)
        plotter.ren_win.Render()
        z = vtkFloatArray()
        plotter.ren_win.GetZbufferData(0, 0, RESOLUTION - 1, RESOLUTION - 1, z)
        depth = vtk_to_numpy(z).reshape(RESOLUTION, RESOLUTION)  # row 0 = bottom of the image
        to_ndc = _matrix(camera.GetCompositeProjectionTransformMatrix(1.0, -1, 1))
    finally:
        plotter.close()

    texel = 2 * radius * 1.01 / RESOLUTION  # world size of one shadow-map texel
    texel_depth = texel / (4 * radius)  # the same distance in z-buffer units
    return [
        _lit(points_w + n * NORMAL_OFFSET_TEXELS * texel, to_ndc, depth, texel_depth)
        * np.clip((n @ light) / TERMINATOR, 0, 1).astype(np.float32)
        for points_w, n in zip(world, normals)
    ]


def _to_world(poly: pv.PolyData, matrix: np.ndarray) -> np.ndarray:
    points = np.asarray(poly.points, dtype=np.float64)
    m = np.asarray(matrix, dtype=np.float64)
    return points @ m[:3, :3].T + m[:3, 3]


def _vertex_normals(poly: pv.PolyData) -> np.ndarray:
    """Area-weighted vertex normals in object space, float32 (vectorised; no VTK filter copying the mesh)."""
    points = np.asarray(poly.points, dtype=np.float64)
    faces = poly.regular_faces
    face_normals = np.cross(points[faces[:, 1]] - points[faces[:, 0]], points[faces[:, 2]] - points[faces[:, 0]])
    normals = np.zeros_like(points)
    for k in range(3):  # bincount scatters the face normals onto their corners far faster than np.add.at
        for axis in range(3):
            normals[:, axis] += np.bincount(faces[:, k], face_normals[:, axis], minlength=len(points))
    return normals.astype(np.float32)


def _to_world_normals(normals: np.ndarray, matrix: np.ndarray) -> np.ndarray:
    world = normals @ np.linalg.inv(np.asarray(matrix, dtype=np.float64)[:3, :3])  # inverse-transpose
    length = np.linalg.norm(world, axis=1, keepdims=True)
    return np.divide(world, length, out=np.zeros_like(world), where=length > 0)


def _matrix(vtk_matrix) -> np.ndarray:
    return np.array([[vtk_matrix.GetElement(i, j) for j in range(4)] for i in range(4)])


def _lit(points: np.ndarray, to_ndc: np.ndarray, depth: np.ndarray, texel_depth: float) -> np.ndarray:
    ndc = points @ to_ndc[:3, :3].T + to_ndc[:3, 3]  # orthographic: w stays 1
    size = depth.shape[0]
    px = np.clip(((ndc[:, 0] + 1) / 2 * size).astype(np.int64), 0, size - 1)
    py = np.clip(((ndc[:, 1] + 1) / 2 * size).astype(np.int64), 0, size - 1)
    z = (ndc[:, 2] + 1) / 2 - BIAS_TEXELS * texel_depth  # z-buffer value of the vertex, nudged toward the sun
    lit = np.zeros(len(points), np.float32)
    offsets = range(-PCF_RADIUS, PCF_RADIUS + 1)
    for dy in offsets:
        rows = np.clip(py + dy, 0, size - 1)
        for dx in offsets:
            lit += z <= depth[rows, np.clip(px + dx, 0, size - 1)]
    return lit / len(offsets) ** 2
