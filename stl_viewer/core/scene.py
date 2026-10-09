from dataclasses import dataclass, field

import numpy as np

from .mesh import Mesh


@dataclass
class SceneObject:
    name: str
    mesh: Mesh
    transform: np.ndarray = field(default_factory=lambda: np.eye(4))  # object -> world, 4x4
    kind: str = "model"  # "model" | "support"
    visible: bool = True
    source: str = ""
    parent: "SceneObject | None" = field(default=None, repr=False, compare=False)  # e.g. the model a support set belongs to


@dataclass
class Plate:
    """Build volume: size (x, y, z) in mm, centred on position in XY, standing on its Z."""

    size: tuple[float, float, float]
    position: tuple[float, float, float] = (0.0, 0.0, 0.0)


@dataclass
class Scene:
    objects: list[SceneObject] = field(default_factory=list)
    plates: list[Plate] = field(default_factory=list)
    skipped: list[str] = field(default_factory=list)  # "<name>: <reason>" for objects that could not be loaded
