"""Generate MeshPeek's icon (icon.png 256 px, icon.ico multi-size) with Pillow.

    python make_icon.py    # writes icon.png and icon.ico next to this script

A faceted low-poly hexagon in teal, lit from the top left, with an orange vertex dot, on a dark rounded tile.
"""

import math
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
S = 1024  # draw large, downsample for smooth edges
ORANGE = (232, 162, 74)
TEAL = (96, 190, 214)


def point(cx, cy, r, deg):
    a = math.radians(deg)
    return (cx + r * math.cos(a), cy + r * math.sin(a))


def shade(color, k):
    return tuple(max(0, min(255, int(v * k))) for v in color) + (255,)


def main():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((40, 40, S - 40, S - 40), radius=190, fill=(43, 46, 52, 255))

    cx, cy, radius = S / 2, S / 2, 330
    outer = [point(cx, cy, radius, -90 + 60 * i) for i in range(6)]
    inner = [point(cx, cy, radius * 0.52, -60 + 120 * i) for i in range(3)]
    # (triangle, brightness): facets facing the light are brighter
    facets = [
        ([outer[0], outer[1], inner[0]], 1.25),
        ([outer[1], outer[2], inner[0]], 1.0),
        ([outer[2], outer[3], inner[1]], 0.78),
        ([outer[3], outer[4], inner[1]], 0.62),
        ([outer[4], outer[5], inner[2]], 0.9),
        ([outer[5], outer[0], inner[2]], 1.12),
        ([inner[0], inner[1], inner[2]], 1.05),
        ([outer[2], inner[1], inner[0]], 0.88),
        ([outer[4], inner[2], inner[1]], 0.72),
        ([outer[0], inner[0], inner[2]], 1.18),
    ]
    for polygon, k in facets:
        d.polygon(polygon, fill=shade(TEAL, k))
    for polygon, _ in facets:
        d.line(polygon + [polygon[0]], fill=(30, 56, 66, 255), width=8, joint="curve")
    x, y = inner[0]
    d.ellipse((x - 34, y - 34, x + 34, y + 34), fill=ORANGE + (255,))

    big = img.resize((256, 256), Image.LANCZOS)
    big.save(HERE / "icon.png")
    big.save(HERE / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print("wrote icon.png and icon.ico")


if __name__ == "__main__":
    main()
