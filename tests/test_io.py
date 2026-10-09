from pathlib import Path

import numpy as np
import pytest

from stl_viewer.core.io import LoadOptions, load, load_mesh
from stl_viewer.core.io.lys import codec, loader, mesh_bin, supports

ROOT = Path(__file__).resolve().parents[1]
CUBE_LYS = ROOT / "cube.lys"
LYD_LYS = ROOT / "Base_110mm_BodyBottom_110.lys"
BUST_LYS = ROOT / "Favorite_BUST - Lychee.lys"

TRIANGLE = np.array([[0, 0, 0], [1, 0, 0], [0, 1, 0]], dtype=np.float32)


def write_binary_stl(path, corners):
    record = np.zeros(len(corners) // 3, dtype=[("n", "<f4", 3), ("v", "<f4", (3, 3)), ("a", "<u2")])
    record["v"] = corners.reshape(-1, 3, 3)
    with open(path, "wb") as f:
        f.write(b"solid looks-like-ascii".ljust(80, b" "))  # binary file whose header starts with "solid"
        f.write(np.uint32(len(record)).tobytes())
        f.write(record.tobytes())


def test_binary_stl(tmp_path):
    path = tmp_path / "t.stl"
    write_binary_stl(path, np.concatenate([TRIANGLE, TRIANGLE + 2]))
    mesh = load_mesh(path)
    assert mesh.n_faces == 2
    np.testing.assert_allclose(mesh.vertices[3:6], TRIANGLE + 2)


def test_ascii_stl(tmp_path):
    path = tmp_path / "t.stl"
    facet = "facet normal 0 0 1\nouter loop\n" + "".join(f"vertex {x} {y} {z}\n" for x, y, z in TRIANGLE) + "endloop\nendfacet\n"
    path.write_text(f"solid t\n{facet}endsolid t\n")
    assert load_mesh(path).n_faces == 1


def test_obj_quad_is_triangulated(tmp_path):
    path = tmp_path / "t.obj"
    path.write_text("v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n")
    assert load_mesh(path).n_faces == 2


def test_ply(tmp_path):
    path = tmp_path / "t.ply"
    path.write_text(
        "ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\n"
        "element face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n3 0 1 2\n"
    )
    assert load_mesh(path).n_faces == 1


def test_single_mesh_scene_keeps_file_coordinates(tmp_path):
    path = tmp_path / "part.stl"
    write_binary_stl(path, TRIANGLE + 100)
    scene = load(path)
    assert [o.name for o in scene.objects] == ["part"]
    np.testing.assert_array_equal(scene.objects[0].transform, np.eye(4))


def test_cipher_roundtrip():
    data = bytes(range(256)) * 3
    key = np.resize(codec._KEY, len(data))
    enciphered = ((np.frombuffer(data, np.uint8) + key) & 0xFF).astype(np.uint8).tobytes()
    assert codec.decipher(enciphered) == data


def test_mesh_bin_rejects_wrong_size():
    raw = np.array([2, 12, 3, 9, 0], "<u4").tobytes() + np.arange(3, dtype="<u4").tobytes()
    with pytest.raises(codec.LysFormatError):
        mesh_bin.parse(raw)


def test_object_matrix_places_lowest_point_at_position_z():
    vertices = np.array([[-1, -1, -1], [1, 1, 1]], dtype=np.float32)
    obj = {"position": {"x": 10, "y": 20, "z": 5}, "rotation": {"x": 0, "y": 90, "z": 0}, "center": {"x": 99, "y": 99, "z": 99}}
    m = loader.object_matrix(obj, vertices)
    world = vertices @ m[:3, :3].T + m[:3, 3]
    assert world[:, 2].min() == pytest.approx(5)  # lift height
    np.testing.assert_allclose(world.mean(0)[:2], [10, 20], atol=1e-6)  # `center` is ignored


def test_frustums_are_closed_and_vectorised():
    mesh = supports.frustums(np.zeros((2, 3)), np.array([[0, 0, 1.0], [0, 0, 0]]), np.ones(2), np.ones(2))
    assert mesh.n_faces == 4 * supports.SIDES  # zero-length segment dropped
    assert mesh.faces.max() < len(mesh.vertices)


@pytest.mark.skipif(not CUBE_LYS.exists(), reason="sample file not present")
def test_cube_lys():
    scene = load(CUBE_LYS)
    names = [o.name for o in scene.objects]
    assert names[:2] == ["cube", "Supports"] and scene.objects[1].parent is scene.objects[0]
    cube = scene.objects[names.index("cube")]
    assert cube.mesh.n_faces == 12
    np.testing.assert_allclose(cube.mesh.vertices.min(0), [-5, -5, -5])
    assert scene.plates[0].size == (143.43, 89.6, 175.0)


@pytest.mark.skipif(not LYD_LYS.exists(), reason="sample file not present")
def test_lyd_objects_are_skipped():
    scene = load(LYD_LYS)
    assert scene.objects == []  # supports of skipped models are skipped too
    assert len(scene.skipped) == 9 and all(".lyd" in s for s in scene.skipped)


def test_ply_vertex_colors(tmp_path):
    path = tmp_path / "c.ply"
    path.write_text(
        "ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\n"
        "property uchar red\nproperty uchar green\nproperty uchar blue\n"
        "element face 1\nproperty list uchar int vertex_indices\nend_header\n"
        "0 0 0 255 0 0\n1 0 0 0 255 0\n0 1 0 0 0 255\n3 0 1 2\n"
    )
    mesh = load_mesh(path)
    np.testing.assert_array_equal(mesh.colors, [[255, 0, 0], [0, 255, 0], [0, 0, 255]])


def test_stl_has_no_colors(tmp_path):
    path = tmp_path / "t.stl"
    write_binary_stl(path, TRIANGLE)
    assert load_mesh(path).colors is None


@pytest.mark.skipif(not BUST_LYS.exists(), reason="sample file not present")
def test_bust_lys_parts_on_plate_and_supports_reach_it():
    scene = load(BUST_LYS)
    assert [(o.name, o.parent and o.parent.name) for o in scene.objects] == [
        ("Base", None), ("Supports", "Base"), ("Pin", None), ("Supports", "Pin"), ("Bust", None), ("Supports", "Bust")
    ]
    plate = scene.plates[0]
    for obj in scene.objects:
        world = obj.mesh.vertices @ obj.transform[:3, :3].T + obj.transform[:3, 3]
        assert np.all(np.abs(world[:, :2]).max(0) <= np.array(plate.size[:2]) / 2 + 1), obj.name
        if obj.kind == "model":
            assert world[:, 2].min() == pytest.approx(5, abs=1e-3), obj.name  # 5 mm lift in this file
        else:
            assert world[:, 2].min() == pytest.approx(0, abs=1e-6)  # support bases sit on the plate


def test_spheres_are_closed_and_sized():
    mesh = supports.spheres(np.array([[0.0, 0, 0], [10, 0, 0]]), np.array([1.0, 2.0]))
    per_sphere = len(mesh.vertices) // 2
    np.testing.assert_allclose(np.linalg.norm(mesh.vertices[:per_sphere], axis=1), 1.0, atol=1e-6)
    np.testing.assert_allclose(np.linalg.norm(mesh.vertices[per_sphere:] - [10, 0, 0], axis=1), 2.0, atol=1e-5)
    edges = np.sort(np.concatenate([mesh.faces[:, [0, 1]], mesh.faces[:, [1, 2]], mesh.faces[:, [2, 0]]]), axis=1)
    _, counts = np.unique(edges, axis=0, return_counts=True)
    assert np.all(counts == 2)  # watertight: every edge shared by exactly two triangles


@pytest.mark.skipif(not BUST_LYS.exists(), reason="sample file not present")
def test_support_detail_changes_support_triangles():
    def support_faces(sides):
        return sum(o.mesh.n_faces for o in load(BUST_LYS, LoadOptions(support_sides=sides)).objects if o.kind == "support")

    assert support_faces(6) < support_faces(12) < support_faces(24)


def test_obj_blender_style_with_empty_material_and_corner_formats(tmp_path):
    path = tmp_path / "b.obj"
    path.write_text(
        "# Blender\nmtllib b.mtl\no thing\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvn 0 0 1\ns 0\nusemtl \n"
        "f 1/1/1 2/1/1 3/1/1\n"  # v/vt/vn
        "f 1//1 3//1 4//1\n"  # v//vn
        "f\t-4/1  -2/1 -1/1 \n"  # v/vt, negative indices, tab and repeated spaces
        "l 1 2\n"
    )
    mesh = load_mesh(path)
    np.testing.assert_array_equal(mesh.faces, [[0, 1, 2], [0, 2, 3], [0, 2, 3]])


def test_obj_polygon_fan_and_vertex_colors(tmp_path):
    path = tmp_path / "p.obj"
    path.write_text(
        "v 0 0 0 1 0 0\nv 1 0 0 0 1 0\nv 1 1 0 0 0 1\nv 0.5 1.5 0 1 1 1\nv 0 1 0 0 0 0\n"
        "f 1 2 3 4 5\nf 1 2 3\n"
    )
    mesh = load_mesh(path)
    expected = [[0, 1, 2], [0, 1, 2], [0, 2, 3], [0, 3, 4]]  # the triangle, plus the pentagon fanned from corner 1
    assert sorted(mesh.faces.tolist()) == expected
    np.testing.assert_array_equal(mesh.colors[:2], [[255, 0, 0], [0, 255, 0]])


def test_obj_bad_index_is_reported(tmp_path):
    path = tmp_path / "bad.obj"
    path.write_text("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 9\n")
    with pytest.raises(ValueError, match="does not exist"):
        load_mesh(path)


def test_obj_y_up_is_rotated_to_z_up(tmp_path):
    path = tmp_path / "up.obj"
    path.write_text("v 0 0 0\nv 1 0 0\nv 0 5 0\nf 1 2 3\n")  # tall along Y
    for y_up, expected_top in [(True, [0, 0, 5]), (False, [0, 5, 0])]:
        obj = load(path, LoadOptions(obj_y_up=y_up)).objects[0]
        top = obj.transform[:3, :3] @ obj.mesh.vertices[2] + obj.transform[:3, 3]
        np.testing.assert_allclose(top, expected_top, atol=1e-6)


def test_y_up_option_only_affects_obj(tmp_path):
    path = tmp_path / "t.stl"
    write_binary_stl(path, TRIANGLE)
    np.testing.assert_array_equal(load(path, LoadOptions(obj_y_up=True)).objects[0].transform, np.eye(4))


def test_joint_sphere_keeps_only_the_outside_of_the_bend():
    centre = np.zeros((1, 3))
    up, side = np.array([[0.0, 0, 1]]), np.array([[1.0, 0, 0]])
    mesh = supports.joint_spheres(centre, np.array([1.0]), up, side, sides=24)
    assert 0 < mesh.n_faces
    corners = mesh.vertices[mesh.faces]  # (faces, 3, 3)
    assert np.all((corners[..., 2] < 1e-6).any(1) & (corners[..., 0] < 1e-6).any(1))  # nothing fully inside an arm
    straight = supports.joint_spheres(centre, np.array([1.0]), up, -up, sides=24)
    assert straight.n_faces == 0  # a straight joint is completely covered


def test_brace_is_a_plain_cylinder():
    record = {"allIds": ["b"], "byId": {"b": {
        "objectIdTip": "o", "objectIdBase": "o", "isBaseTip": True, "parentTipId": "p", "parentBaseId": "q",
        "tip": {"x": 0, "y": 0, "z": 10}, "tipNormal": {"x": 0, "y": 0, "z": -1}, "base": {"x": 5, "y": 0, "z": 5},
        "settings": {"tip": {"diameter": 1.0, "pointDiameter": 0.3, "length": 2}},
    }}}
    mesh = supports.build(record, {"o": np.eye(4)}, {"o": {}}, sides=8)["o"]
    assert mesh.n_faces == 4 * 8  # exactly one capped cylinder: no tip cone, no joint sphere
    radii = np.linalg.norm(np.cross(mesh.vertices - [0, 0, 10], [5, 0, -5]) / np.linalg.norm([5, 0, -5]), axis=1)
    np.testing.assert_allclose(radii[radii > 1e-6], 0.5, atol=1e-6)  # constant radius along the brace
