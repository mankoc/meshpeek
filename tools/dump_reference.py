"""Write golden reference JSON for the Rust port from the Python loaders.

    python tools/dump_reference.py            # writes samples/reference/*.json

For every sample file (and each load option that changes the result) it records, per object: name, kind,
parent, vertex/face counts, object-space bounds and centroid, the object->world transform and the world
bounds. The Rust tests compare their loaders against these with a small float tolerance.
"""

import json
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from stl_viewer.core.io import LoadOptions, load  # noqa: E402

SAMPLES = ROOT / "samples"
OUT = SAMPLES / "reference"

CASES = [  # (sample file, reference name, options)
    *[("Favorite_BUST - Lychee.lys", f"bust_sides{n}", LoadOptions(support_sides=n)) for n in (6, 12, 24, 48)],
    ("Pilone_DX.obj", "pilone_yup", LoadOptions(obj_y_up=True)),
    ("Pilone_DX.obj", "pilone_zup", LoadOptions(obj_y_up=False)),
]


def describe(scene) -> dict:
    objects = []
    for obj in scene.objects:
        v = obj.mesh.vertices.astype(np.float64)
        m = np.asarray(obj.transform, dtype=np.float64)
        world = v @ m[:3, :3].T + m[:3, 3]
        objects.append({
            "name": obj.name,
            "kind": obj.kind,
            "parent": obj.parent.name if obj.parent is not None else None,
            "n_vertices": int(len(v)),
            "n_faces": int(obj.mesh.n_faces),
            "has_colors": obj.mesh.colors is not None,
            "bounds_min": v.min(0).tolist(),
            "bounds_max": v.max(0).tolist(),
            "centroid": v.mean(0).tolist(),
            "transform": m.tolist(),  # row-major 4x4
            "world_min": world.min(0).tolist(),
            "world_max": world.max(0).tolist(),
        })
    return {
        "objects": objects,
        "skipped": scene.skipped,
        "plates": [{"size": list(p.size), "position": list(p.position)} for p in scene.plates],
    }


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for file, name, options in CASES:
        path = SAMPLES / file
        if not path.exists():
            print(f"skip {file} (missing)")
            continue
        data = {"file": file, "options": {"support_sides": options.support_sides, "obj_y_up": options.obj_y_up}}
        data.update(describe(load(path, options)))
        (OUT / f"{name}.json").write_text(json.dumps(data, indent=1))
        print(f"wrote {name}.json: {len(data['objects'])} objects")


if __name__ == "__main__":
    main()
