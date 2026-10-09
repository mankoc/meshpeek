"""3D view: the only module that turns core meshes into VTK actors."""

import numpy as np
import pyvista as pv
from PySide6.QtCore import QTimer
from pyvistaqt import QtInteractor
from vtkmodules.util.numpy_support import numpy_to_vtk
from vtkmodules.vtkCommonCore import vtkLookupTable
from vtkmodules.vtkCommonDataModel import vtkCellArray
from vtkmodules.vtkRenderingCore import vtkPolyDataMapper

from ..core.mesh import Mesh
from ..core.scene import Plate, SceneObject
from . import shadows as shadow_maps
from .settings import MATERIALS, ViewSettings

SUN_DIRECTION = np.array([-0.45, -0.6, 1.0]) / np.linalg.norm([-0.45, -0.6, 1.0])  # from the scene toward the light
SHADOW_COLOR = "#2c3036"  # plate colour in shade
SHADOW_LIFT = 0.02  # mm above the ground, so shadows do not z-fight with the plate
SHADE = 0.6  # brightness of model and support surfaces in shadow, relative to their colour


def shadow_matrix(light_direction, ground_z: float) -> np.ndarray:
    """4x4 matrix moving every point along `light_direction` onto the plane z = ground_z."""
    lx, ly, lz = light_direction
    matrix = np.eye(4)
    matrix[0, 2], matrix[0, 3] = -lx / lz, lx / lz * ground_z
    matrix[1, 2], matrix[1, 3] = -ly / lz, ly / lz * ground_z
    matrix[2, 2], matrix[2, 3] = 0.0, ground_z
    return matrix


def to_polydata(mesh: Mesh) -> pv.PolyData:
    """Wrap the mesh arrays for VTK without copying them.

    Points and triangle indices are shared with the numpy arrays (VTK keeps them alive), using VTK's
    32-bit cell storage, so a loaded mesh exists once in memory rather than twice. Safe to call off the
    UI thread: it only builds data objects.
    """
    faces = np.ascontiguousarray(mesh.faces, dtype=np.int32).reshape(-1)
    offsets = np.arange(0, faces.size + 1, 3, dtype=np.int32)
    cells = vtkCellArray()
    cells.SetData(numpy_to_vtk(offsets, deep=False), numpy_to_vtk(faces, deep=False))
    poly = pv.PolyData()
    poly.SetPoints(pv.vtk_points(np.ascontiguousarray(mesh.vertices, dtype=np.float32), deep=False))
    poly.SetPolys(cells)
    if mesh.colors is not None:
        poly.point_data["RGB"] = mesh.colors
    return poly


class Viewport(QtInteractor):
    def __init__(self, settings: ViewSettings, parent=None):
        super().__init__(parent)
        self.set_background("#25272b", top="#4b505a")
        self.enable_anti_aliasing("msaa")  # hardware multisampling: far cheaper than SSAA on modest GPUs
        self.add_camera_orientation_widget()  # click a face/axis to snap the view
        self.add_axes_at_origin(labels_off=True, line_width=1)
        self._actors: dict[int, object] = {}  # id(SceneObject) -> actor
        self._kinds: dict[int, str] = {}  # id(SceneObject) -> "model" | "support"
        self._default_colored: set[int] = set()  # ids of models drawn in the default colour
        self._plates: set[tuple] = set()  # (size, position) of plates already drawn
        self._shadows: dict[int, object] = {}  # id(SceneObject) -> its flattened shadow actor
        self._normal_cache: dict[str, np.ndarray] = {}  # mesh VTK address -> vertex normals, for re-bakes
        self._bake_pending = False
        self.shadows = False
        self.settings = settings
        self.wireframe = False
        self.focus: SceneObject | None = None  # object the camera is centred on and orbits, if any

    def add_object(self, obj: SceneObject, poly: pv.PolyData | None = None):
        """Add an object; pass `poly` if `to_polydata` was already run (e.g. on the loading thread)."""
        has_colors = obj.mesh.colors is not None
        mapper = vtkPolyDataMapper()
        mapper.SetInputData(poly if poly is not None else to_polydata(obj.mesh))
        mapper.SetStatic(True)  # data never changes after loading: skip the pipeline check on every frame
        mapper.SetScalarVisibility(has_colors)
        if has_colors:
            mapper.SetScalarModeToUsePointFieldData()
            mapper.SelectColorArray("RGB")
            mapper.SetColorModeToDirectScalars()
        actor = pv.Actor(mapper=mapper)
        actor.prop.color = self.settings.support_color if obj.kind == "support" else self.settings.model_color
        actor.prop.style = "wireframe" if self.wireframe else "surface"
        self.add_actor(actor, reset_camera=False, name=f"obj-{id(obj)}")
        actor.user_matrix = obj.transform
        actor.SetVisibility(obj.visible)
        self._actors[id(obj)] = actor
        self._kinds[id(obj)] = obj.kind
        if obj.kind == "model" and not has_colors:
            self._default_colored.add(id(obj))
        self._apply_material(actor, obj.kind)
        if self.shadows:
            self._add_shadow(id(obj))
            self._update_shadows()  # the ground may have moved down
            self._schedule_bake()
        return actor

    def _color_of(self, key: int) -> str:
        return self.settings.support_color if self._kinds[key] == "support" else self.settings.model_color

    def _paint(self, key: int):
        """Point the object's mapper at the right colours: plain colour or file RGB, either one darkened by
        the baked shadows ("lit" per vertex) while shadows are on."""
        mapper = self._actors[key].GetMapper()
        data = mapper.GetInput().GetPointData()
        shadowed = self.shadows and data.HasArray("lit")
        if data.HasArray("RGB"):
            mapper.SetScalarVisibility(True)
            mapper.SetScalarModeToUsePointFieldData()
            mapper.SelectColorArray("RGB_lit" if shadowed else "RGB")
            mapper.SetColorModeToDirectScalars()
        elif shadowed:
            color = np.array(pv.Color(self._color_of(key)).float_rgb)
            lut = vtkLookupTable()
            lut.SetNumberOfTableValues(2)
            lut.SetTableRange(0.0, 1.0)
            lut.SetTableValue(0, *(color * SHADE), 1.0)
            lut.SetTableValue(1, *color, 1.0)
            lut.Build()
            mapper.SetLookupTable(lut)
            mapper.SetUseLookupTableScalarRange(True)
            mapper.SetScalarVisibility(True)
            mapper.SetScalarModeToUsePointFieldData()
            mapper.SelectColorArray("lit")
            mapper.SetColorModeToMapScalars()
            mapper.SetInterpolateScalarsBeforeMapping(True)  # smooth gradient across each triangle
        else:
            mapper.SetScalarVisibility(False)
        mapper.Modified()  # the mapper is static: tell it its colours changed

    def _apply_material(self, actor, kind: str):
        material = self.settings.support_material if kind == "support" else self.settings.material
        for name, value in MATERIALS[material].items():
            setattr(actor.prop, name, value)

    def apply_settings(self, settings: ViewSettings):
        self.settings = settings
        for key, actor in self._actors.items():
            kind = self._kinds[key]
            self._apply_material(actor, kind)
            if kind == "support":
                actor.prop.color = settings.support_color
            elif key in self._default_colored:
                actor.prop.color = settings.model_color
            if self.shadows:
                self._paint(key)  # shadow colour ramps are built from the object colour
        self.render()

    def set_visible(self, obj: SceneObject, visible: bool):
        obj.visible = visible
        self._actors[id(obj)].SetVisibility(visible)
        if id(obj) in self._shadows:
            self._shadows[id(obj)].SetVisibility(visible)
        self._schedule_bake()  # hidden objects cast no shadows
        self.render()

    def add_plate(self, plate: Plate):
        key = (plate.size, plate.position)
        if key in self._plates:  # several files on the same printer share one plate
            return
        self._plates.add(key)
        (sx, sy, sz), (px, py, pz) = plate.size, plate.position
        bounds = (px - sx / 2, px + sx / 2, py - sy / 2, py + sy / 2, pz, pz + sz)
        floor = pv.Plane(center=(px, py, pz), i_size=sx, j_size=sy, i_resolution=1, j_resolution=1)
        self.add_mesh(floor, color="#7a8494", opacity=0.35, reset_camera=False, pickable=False)
        self.add_mesh(pv.Box(bounds).outline(), color="#7a8494", opacity=0.6, reset_camera=False, pickable=False)
        self._update_shadows()

    def set_shadows(self, on: bool):
        """Shadows from a fixed sun (SUN_DIRECTION), in two parts:

        - on the plate: every object drawn again, flattened onto the ground (planar shadows);
        - on models and supports: a shadow map baked once per scene change (see `shadows.py`) and shown
          as per-vertex shading, so frames cost no more than without shadows.
        While on, the sun plus a weak headlight replace the camera-following light kit so shading and
        shadows agree. (VTK's own shadow-map pass assumes a square window and misplaces shadows.)
        """
        self.shadows = on
        self.remove_all_lights()
        if on:
            sun = pv.Light(light_type="scene light", intensity=0.8)
            sun.positional = False
            sun.position, sun.focal_point = tuple(SUN_DIRECTION), (0.0, 0.0, 0.0)
            self.add_light(sun)
            self.add_light(pv.Light(light_type="headlight", intensity=0.45))
            for key in self._actors:
                self._add_shadow(key)
            self._update_shadows()
            self._schedule_bake()
        else:
            self.enable_lightkit()
            for shadow in self._shadows.values():
                self.remove_actor(shadow, render=False)
            self._shadows.clear()
            for key, actor in self._actors.items():
                data = actor.GetMapper().GetInput().GetPointData()
                data.RemoveArray("lit")
                data.RemoveArray("RGB_lit")
                self._paint(key)
            self._normal_cache.clear()
        self.render()

    def _schedule_bake(self):
        """Coalesce bake requests (e.g. every object of a file being added) into one, after the event."""
        if self.shadows and not self._bake_pending:
            self._bake_pending = True
            QTimer.singleShot(0, self._bake)

    def _bake(self):
        self._bake_pending = False
        if not self.shadows:
            return
        keys = [key for key, actor in self._actors.items() if actor.GetVisibility()]
        polys = [self._actors[key].GetMapper().GetInput() for key in keys]
        matrices = [np.asarray(self._actors[key].user_matrix) for key in keys]
        factors = shadow_maps.bake(list(zip(polys, matrices)), SUN_DIRECTION, self._normal_cache)
        for key, poly, lit in zip(keys, polys, factors):
            poly.point_data["lit"] = lit
            if "RGB" in poly.point_data:
                shade = SHADE + (1 - SHADE) * lit[:, None]
                poly.point_data["RGB_lit"] = (np.asarray(poly.point_data["RGB"]) * shade).astype(np.uint8)
            self._paint(key)
        self.render()

    def _add_shadow(self, key: int):
        actor = self._actors[key]
        mapper = vtkPolyDataMapper()
        mapper.SetInputData(actor.GetMapper().GetInput())  # same data, and VTK shares the GPU buffers
        mapper.SetStatic(True)
        mapper.SetScalarVisibility(False)
        shadow = pv.Actor(mapper=mapper)
        shadow.prop.color = SHADOW_COLOR
        shadow.prop.lighting = False
        shadow.SetPickable(False)
        shadow.SetVisibility(actor.GetVisibility())
        self.add_actor(shadow, reset_camera=False, render=False, name=f"shadow-{key}")
        self._shadows[key] = shadow

    def _update_shadows(self):
        """Re-flatten all shadows onto the current ground: the plate, or the lowest point of the scene."""
        if not self._shadows:
            return
        if self._plates:
            ground = min(position[2] for _, position in self._plates)
        else:
            ground = min(actor.GetBounds()[4] for actor in self._actors.values())
        flatten = shadow_matrix(SUN_DIRECTION, ground + SHADOW_LIFT)
        for key, shadow in self._shadows.items():
            shadow.user_matrix = flatten @ np.asarray(self._actors[key].user_matrix)

    def set_wireframe(self, on: bool):
        self.wireframe = on
        for actor in self._actors.values():
            actor.prop.style = "wireframe" if on else "surface"
        self.render()

    def clear_scene(self):
        self.clear_actors()
        self._actors.clear()
        self._kinds.clear()
        self._default_colored.clear()
        self._plates.clear()
        self._shadows.clear()
        self._normal_cache.clear()
        self.focus = None
        self.add_axes_at_origin(labels_off=True, line_width=1)
        self.render()

    def fit(self):
        """Frame the whole scene and stop orbiting a centred object."""
        self.focus = None
        self.reset_camera()
        self.render()

    def center_on(self, obj: SceneObject):
        """Aim the camera at one object and zoom to fit it, keeping the viewing direction.

        The camera orbits its focal point, so rotation now pivots on this object; standard views keep it
        framed until `fit` is used.
        """
        self.focus = obj
        self._frame_focus()

    def show_view(self, view, **kwargs):
        """Apply a standard view (e.g. `self.view_xz`), framing the centred object if there is one."""
        view(**kwargs)
        if self.focus is not None:
            self._frame_focus()

    def _frame_focus(self):
        self.renderer.ResetCamera(self._actors[id(self.focus)].GetBounds())  # bounds include the transform
        self.camera_set = True  # otherwise pyvista resets the camera to the whole scene on the next render
        self.render()
