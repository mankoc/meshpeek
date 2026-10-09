import numpy as np

from stl_viewer.gui.viewport import shadow_matrix


def test_shadow_matrix_projects_along_the_light_onto_the_ground():
    light = np.array([-0.5, 0.25, 1.0])
    m = shadow_matrix(light, ground_z=2.0)
    point = np.array([10.0, 4.0, 12.0, 1.0])
    projected = m @ point
    assert projected[2] == 2.0  # lands on the ground plane
    offset = point[:3] - projected[:3]
    np.testing.assert_allclose(np.cross(offset, light), 0, atol=1e-12)  # moved straight along the light
    np.testing.assert_allclose(m @ [3.0, -1.0, 2.0, 1.0], [3.0, -1.0, 2.0, 1.0])  # points on the ground stay put


def test_bake_shadows_the_floor_under_a_box_and_lights_the_rest():
    import pyvista as pv

    from stl_viewer.gui import shadows

    floor = pv.Plane(i_size=100, j_size=100, i_resolution=100, j_resolution=100).triangulate()
    box = pv.Cube(center=(0, 0, 20), x_length=20, y_length=20, z_length=4).triangulate()
    lit_floor, lit_box = shadows.bake([(floor, np.eye(4)), (box, np.eye(4))], np.array([0.0, 0.0, 1.0]))
    xy = np.asarray(floor.points)[:, :2]
    under = np.all(np.abs(xy) < 8, axis=1)  # well inside the box's footprint
    clear = np.any(np.abs(xy) > 12, axis=1)  # well outside it
    assert lit_floor[under].max() == 0.0
    assert lit_floor[clear].min() == 1.0
    top = np.asarray(box.points)[:, 2] > 21
    assert lit_box[top].min() > 0.99  # the box's sun-facing top is lit
