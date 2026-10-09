"""3D view: the only module that turns core meshes into VTK actors."""

import pyvista as pv
from pyvistaqt import QtInteractor

from ..core.mesh import Mesh
from ..core.scene import Plate, SceneObject
from .settings import MATERIALS, ViewSettings


def to_polydata(mesh: Mesh) -> pv.PolyData:
    poly = pv.PolyData.from_regular_faces(mesh.vertices, mesh.faces)
    if mesh.colors is not None:
        poly.point_data["RGB"] = mesh.colors
    return poly


class Viewport(QtInteractor):
    def __init__(self, settings: ViewSettings, parent=None):
        super().__init__(parent)
        self.set_background("#25272b", top="#4b505a")
        self.enable_anti_aliasing("ssaa")
        self.add_camera_orientation_widget()  # click a face/axis to snap the view
        self.add_axes_at_origin(labels_off=True, line_width=1)
        self._actors: dict[int, object] = {}  # id(SceneObject) -> actor
        self._kinds: dict[int, str] = {}  # id(SceneObject) -> "model" | "support"
        self._default_colored: set[int] = set()  # ids of models drawn in the default colour
        self.settings = settings
        self.wireframe = False
        self.focus: SceneObject | None = None  # object the camera is centred on and orbits, if any

    def add_object(self, obj: SceneObject):
        has_colors = obj.mesh.colors is not None
        actor = self.add_mesh(
            to_polydata(obj.mesh),
            scalars="RGB" if has_colors else None,
            rgb=has_colors,
            color=self.settings.support_color if obj.kind == "support" else self.settings.model_color,
            style="wireframe" if self.wireframe else "surface",
            reset_camera=False,
            name=f"obj-{id(obj)}",
        )
        actor.user_matrix = obj.transform
        actor.SetVisibility(obj.visible)
        self._actors[id(obj)] = actor
        self._kinds[id(obj)] = obj.kind
        if obj.kind == "model" and not has_colors:
            self._default_colored.add(id(obj))
        self._apply_material(actor, obj.kind)
        return actor

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
        self.render()

    def set_visible(self, obj: SceneObject, visible: bool):
        obj.visible = visible
        self._actors[id(obj)].SetVisibility(visible)
        self.render()

    def add_plate(self, plate: Plate):
        (sx, sy, sz), (px, py, pz) = plate.size, plate.position
        bounds = (px - sx / 2, px + sx / 2, py - sy / 2, py + sy / 2, pz, pz + sz)
        floor = pv.Plane(center=(px, py, pz), i_size=sx, j_size=sy, i_resolution=1, j_resolution=1)
        self.add_mesh(floor, color="#7a8494", opacity=0.35, reset_camera=False, pickable=False)
        self.add_mesh(pv.Box(bounds).outline(), color="#7a8494", opacity=0.6, reset_camera=False, pickable=False)

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
