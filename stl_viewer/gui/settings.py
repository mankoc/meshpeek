"""User preferences for rendering, persisted with QSettings."""

import os
from dataclasses import asdict, dataclass
from pathlib import Path

from PySide6.QtCore import QSettings

from ..core.io import LoadOptions

MAX_RECENT = 10


def _store() -> QSettings:
    return QSettings("STLViewer", "STLViewer")

# VTK surface properties per material preset (applied to every object's actor property).
MATERIALS = {
    "Matte": dict(interpolation="flat", ambient=0.15, diffuse=0.9, specular=0.0, specular_power=1.0),
    "Plastic": dict(interpolation="flat", ambient=0.1, diffuse=0.8, specular=0.3, specular_power=20.0),
    "Glossy": dict(interpolation="flat", ambient=0.1, diffuse=0.7, specular=1.0, specular_power=10.0),
    # Phong rather than PBR: PBR metal only looks right with an environment map to reflect.
    "Metal": dict(interpolation="flat", ambient=0.3, diffuse=0.45, specular=1.0, specular_power=12.0),
}

# Facets around .lys support columns and their joint spheres.
SUPPORT_DETAIL = {"Low": 6, "Medium": 12, "High": 24, "Very high": 48}


@dataclass
class ViewSettings:
    material: str = "Plastic"  # model material
    model_color: str = "#c3ccd9"  # used when the file has no colours of its own
    support_detail: str = "Medium"
    obj_y_up: bool = True  # Blender, Maya and ZBrush export OBJ with Y up
    support_material: str = "Plastic"
    support_color: str = "#e8a24a"

    @classmethod
    def load(cls) -> "ViewSettings":
        store = _store()
        defaults = cls()
        settings = cls(**{k: store.value(k, v, type=type(v)) for k, v in asdict(defaults).items()})
        for key in ("material", "support_material"):
            if getattr(settings, key) not in MATERIALS:
                setattr(settings, key, getattr(defaults, key))
        if settings.support_detail not in SUPPORT_DETAIL:
            settings.support_detail = defaults.support_detail
        return settings

    def load_options(self) -> LoadOptions:
        """The part of the settings that affects how files are loaded (changing it needs a reload)."""
        return LoadOptions(support_sides=SUPPORT_DETAIL[self.support_detail], obj_y_up=self.obj_y_up)

    def save(self):
        store = _store()
        for key, value in asdict(self).items():
            store.setValue(key, value)


def recent_files() -> list[str]:
    value = _store().value("recent_files", [])
    return [value] if isinstance(value, str) else list(value or [])  # the registry returns one entry as a str


def add_recent_file(path) -> None:
    path = str(Path(path).resolve())
    others = [f for f in recent_files() if os.path.normcase(f) != os.path.normcase(path)]
    _store().setValue("recent_files", [path, *others][:MAX_RECENT])


def clear_recent_files() -> None:
    _store().remove("recent_files")
