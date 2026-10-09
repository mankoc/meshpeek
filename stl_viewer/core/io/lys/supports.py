"""Support geometry from Lychee's parametric support records.

A plate support is drawn as: foot -> vertical column up to the knee (`base.length + base.joinLength` above
the plate, along `baseNormal`) -> angled column to the end of the tip cone -> tip cone touching the model.
A branch support (`isBaseTip`) starts on its parent's column and runs to its own tip cone.
A brace (`parentTipId` set) has its tip on another support instead of the model: plain cylinder, no cone.
Bends (knee, tip cone end) get a sphere so the joints look rounded instead of cut. Only the part of
each sphere outside both adjoining segments is kept; branch starts need none (they sit inside the parent).

Coordinates (checked against sample files): `tip` is in the tip object's space. Branch `base` points are in
the base object's space and lie exactly on the parent column. Plate `base` points are at plate height with
x/y relative to the base object's position.

Not drawn: Lychee's bracing lattice between grid supports. It is generated from bracing presets at
runtime and is not stored in the file.
"""

import numpy as np

from ...mesh import Mesh

SIDES = 12  # default facets around each support column


def joint_scale(sides: int) -> float:
    """Joint spheres must stay inside the columns: an n-sided prism's flat sides sit at r*cos(pi/n),
    so a full-radius sphere would poke out between them. 0.99 keeps the surfaces from z-fighting."""
    return np.cos(np.pi / sides) * 0.99


def _vec(d, default=0.0) -> np.ndarray:
    d = d or {}
    return np.array([d.get("x", default), d.get("y", default), d.get("z", default)], dtype=float)


def _apply(matrix: np.ndarray, point: np.ndarray) -> np.ndarray:
    return matrix[:3, :3] @ point + matrix[:3, 3]


def _unit(v: np.ndarray, fallback) -> np.ndarray:
    length = np.linalg.norm(v)
    return v / length if length > 1e-9 else np.asarray(fallback, dtype=float)


def build(
    supports: dict, transforms: dict[str, np.ndarray], objects: dict[str, dict], sides: int = SIDES
) -> dict[str, Mesh]:
    """World-space support meshes per model: {tip object id: all visible supports touching that object}.

    `transforms` holds the placed objects' matrices; `objects` their Lychee records (position, grid mode).
    `sides` is the facet count around columns and joint spheres (render detail).
    """
    segments: dict[str, tuple[list, list, list, list]] = {}  # tip object id -> p0, p1, r0, r1
    joints: dict[str, tuple[list, list, list, list]] = {}  # tip object id -> centres, radii, arm a, arm b

    for sid in supports.get("allIds", []):
        s = supports["byId"][sid]
        tip_matrix = transforms.get(s.get("objectIdTip"))
        if tip_matrix is None or not s.get("isVisible", True):
            continue
        p0, p1, r0, r1 = segments.setdefault(s["objectIdTip"], ([], [], [], []))
        centres, radii, arms_a, arms_b = joints.setdefault(s["objectIdTip"], ([], [], [], []))

        def segment(a, b, ra, rb):
            p0.append(a), p1.append(b), r0.append(ra), r1.append(rb)

        def joint(point, radius, toward_a, toward_b):  # the two segments leave `point` toward a and b
            centres.append(point), radii.append(radius)
            arms_a.append(_unit(toward_a - point, [0, 0, 1])), arms_b.append(_unit(toward_b - point, [0, 0, -1]))

        settings = s.get("settings") or {}
        tip_cfg, base_cfg = settings.get("tip") or {}, settings.get("base") or {}
        column_radius = tip_cfg.get("diameter", 1.0) / 2

        tip = _apply(tip_matrix, _vec(s.get("tip")))
        brace = bool(s.get("parentTipId"))  # tip rests on another support (checked: exactly on its column)
        if brace:
            tip_end = tip
        else:
            tip_end = tip + _unit(tip_matrix[:3, :3] @ _vec(s.get("tipNormal")), [0, 0, -1]) * tip_cfg.get("length", 3.0)
            segment(tip, tip_end, tip_cfg.get("pointDiameter", 0.4) / 2, column_radius)

        base_id = s.get("objectIdBase") if s.get("objectIdBase") in transforms else s.get("objectIdTip")
        if s.get("isBaseTip"):  # branch: starts on the parent support's column
            branch_start = _apply(transforms[base_id], _vec(s.get("base")))
            segment(branch_start, tip_end, column_radius, column_radius)
            if not brace:
                joint(tip_end, column_radius, tip, branch_start)
            continue

        base_obj = objects[base_id]
        foot = _vec(s.get("base")) + _vec(base_obj.get("position")) * [1.0, 1.0, 0.0]
        up = _unit(_vec(s.get("baseNormal")), [0, 0, 1])
        join_radius = base_cfg.get("joinDiameter", 2 * column_radius) / 2
        foot_height = base_cfg.get("length", 0.5)
        if base_obj.get("isSupportGrid"):  # grid supports stand on Lychee's lattice: small foot instead of a disc
            foot_radius, foot_height = 2 * join_radius, max(foot_height, 0.5)
        else:
            foot_radius = base_cfg.get("diameter", 6.0) / 2
        foot_top = foot + up * foot_height
        knee = foot + up * (base_cfg.get("length", 0.5) + base_cfg.get("joinLength", 0.0))
        segment(foot, foot_top, foot_radius, join_radius)
        if np.dot(knee - foot_top, up) > 0:
            segment(foot_top, knee, join_radius, join_radius)
        else:
            knee = foot_top
        segment(knee, tip_end, join_radius, column_radius)
        if not brace:
            joint(tip_end, column_radius, tip, knee)
        joint(knee, join_radius, foot, tip_end)

    meshes = {}
    for oid, segs in segments.items():
        centres, radii, arms_a, arms_b = map(np.array, joints[oid])
        meshes[oid] = Mesh.merge([
            frustums(*map(np.array, segs), sides),
            joint_spheres(centres, radii * joint_scale(sides), arms_a, arms_b, sides),
        ])
    return meshes


def joint_spheres(centres, radii, arms_a, arms_b, sides: int = SIDES) -> Mesh:
    """Spheres rounding the bends, minus the triangles hidden inside the two adjoining segments.

    The sphere is narrower than the columns, so everything on a segment's side of the joint (positive
    dot product with that segment's direction) is inside that segment. What remains is the wedge on the
    outside of the bend.
    """
    if len(centres) == 0:
        return Mesh(np.zeros((0, 3), np.float32), np.zeros((0, 3), np.int64))
    mesh = spheres(centres, radii, sides, max(2, sides // 2))
    per_sphere = len(mesh.vertices) // len(centres)
    owner = np.repeat(np.arange(len(centres)), per_sphere)
    offset = mesh.vertices - centres[owner]
    tolerance = -1e-6 * radii[owner]
    in_a = np.einsum("ij,ij->i", offset, arms_a[owner]) >= tolerance
    in_b = np.einsum("ij,ij->i", offset, arms_b[owner]) >= tolerance
    visible = ~(in_a[mesh.faces].all(1) | in_b[mesh.faces].all(1))
    used, faces = np.unique(mesh.faces[visible], return_inverse=True)  # drop vertices no face uses any more
    return Mesh(mesh.vertices[used], faces.reshape(-1, 3).astype(np.int64))


def spheres(centres, radii, sides: int = SIDES, rings: int = SIDES // 2) -> Mesh:
    """Low-poly UV spheres, vectorised over all centres."""
    theta = np.linspace(0, np.pi, rings + 1)[1:-1]  # latitudes without the poles
    phi = np.linspace(0, 2 * np.pi, sides, endpoint=False)
    unit = np.concatenate([
        [[0.0, 0.0, 1.0]],
        np.stack([np.outer(np.sin(theta), np.cos(phi)), np.outer(np.sin(theta), np.sin(phi)),
                  np.repeat(np.cos(theta)[:, None], sides, 1)], -1).reshape(-1, 3),
        [[0.0, 0.0, -1.0]],
    ])
    a = np.arange(sides)
    b = (a + 1) % sides
    bottom = len(unit) - 1
    last_ring = 1 + (rings - 2) * sides
    template = [np.stack([np.zeros(sides, int), 1 + b, 1 + a], 1)]
    for k in range(rings - 2):  # quads between consecutive rings
        top, low = 1 + k * sides, 1 + (k + 1) * sides
        template += [np.stack([top + a, top + b, low + b], 1), np.stack([top + a, low + b, low + a], 1)]
    template.append(np.stack([np.full(sides, bottom), last_ring + a, last_ring + b], 1))
    template = np.concatenate(template)

    vertices = centres[:, None, :] + radii[:, None, None] * unit[None]
    faces = template[None] + (np.arange(len(centres)) * len(unit))[:, None, None]
    return Mesh(vertices.reshape(-1, 3).astype(np.float32), faces.reshape(-1, 3).astype(np.int64))


def frustums(p0, p1, r0, r1, sides: int = SIDES) -> Mesh:
    """Capped truncated cones from p0 (radius r0) to p1 (radius r1), vectorised over all segments."""
    axis = p1 - p0
    length = np.linalg.norm(axis, axis=1)
    keep = length > 1e-6
    p0, p1, r0, r1, axis = p0[keep], p1[keep], r0[keep], r1[keep], axis[keep] / length[keep, None]

    ref = np.where(np.abs(axis[:, 2:3]) < 0.9, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])
    u = np.cross(axis, ref)
    u /= np.linalg.norm(u, axis=1)[:, None]
    v = np.cross(axis, u)
    angles = np.linspace(0, 2 * np.pi, sides, endpoint=False)
    ring = np.cos(angles)[None, :, None] * u[:, None, :] + np.sin(angles)[None, :, None] * v[:, None, :]
    vertices = np.concatenate(
        [p0[:, None] + r0[:, None, None] * ring, p1[:, None] + r1[:, None, None] * ring, p0[:, None], p1[:, None]],
        axis=1,
    )

    a = np.arange(sides)
    b = (a + 1) % sides
    bottom_centre, top_centre = 2 * sides, 2 * sides + 1
    template = np.concatenate([
        np.stack([a, b, sides + b], 1),
        np.stack([a, sides + b, sides + a], 1),
        np.stack([np.full(sides, bottom_centre), b, a], 1),
        np.stack([np.full(sides, top_centre), sides + a, sides + b], 1),
    ])
    per_segment = 2 * sides + 2
    faces = template[None] + (np.arange(len(p0)) * per_segment)[:, None, None]
    return Mesh(vertices.reshape(-1, 3).astype(np.float32), faces.reshape(-1, 3).astype(np.int64))
