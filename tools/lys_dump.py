"""Print the members and decoded scene state of a Lychee .lys file.

    python tools/lys_dump.py scene.lys                          # members + top-level keys
    python tools/lys_dump.py scene.lys objects.present.byId.o1  # one subtree as JSON
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from stl_viewer.core.io.lys import codec  # noqa: E402
from stl_viewer.core.io.lys.container import Archive  # noqa: E402


def main(path: str, key: str | None = None):
    with Archive(path) as archive:
        print(f"container: {'ZIP (V1)' if archive.is_zip else 'MNGO (V2+)'}")
        for name in sorted(archive.names):
            print(f"  {name:60s} {len(archive.read(name)):>12,d} bytes")
        state = codec.decode(archive.read("scene.bin"), ciphered=not archive.is_zip)
    if key is None:
        print("state keys:", ", ".join(state))
        return
    for part in key.split("."):
        state = state[part]
    print(json.dumps(state, indent=1, default=lambda b: f"<{len(b)} bytes>" if isinstance(b, bytes) else str(b)))


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    main(*sys.argv[1:])
