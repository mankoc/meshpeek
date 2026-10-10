"""Generate MeshPeek's icon (icon.png 256 px, icon.ico multi-size) with Pillow.

    python make_icon.py    # writes icon.png and icon.ico next to this script

A shaded isometric cube with a light mesh pattern and an orange support column, on a dark rounded tile.
"""

from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
S = 1024  # draw large, downsample for smooth edges


def lerp(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)


def main():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((40, 40, S - 40, S - 40), radius=190, fill=(43, 46, 52, 255))

    cx, cy, r = S / 2, S / 2 - 70, 300  # isometric cube
    top, left, right = (cx, cy - r), (cx - r * 0.87, cy - r / 2), (cx + r * 0.87, cy - r / 2)
    centre = (cx, cy)
    bottom, bottom_left, bottom_right = (cx, cy + r), (cx - r * 0.87, cy + r / 2), (cx + r * 0.87, cy + r / 2)

    # orange support column under the cube, with a foot
    d.polygon([(cx - 22, cy + r - 10), (cx + 22, cy + r - 10), (cx + 22, S - 175), (cx - 22, S - 175)], fill=(232, 162, 74, 255))
    d.ellipse((cx - 85, S - 205, cx + 85, S - 150), fill=(232, 162, 74, 255))

    faces = [
        ([top, right, centre, left], (205, 213, 225)),  # top face, lit
        ([left, centre, bottom, bottom_left], (150, 160, 175)),
        ([centre, right, bottom_right, bottom], (112, 121, 136)),
    ]
    for polygon, color in faces:
        d.polygon(polygon, fill=color + (255,))
    # mesh pattern: a few triangle edges on each face
    line = (80, 88, 102, 255)
    for polygon, _ in faces:
        a, b, c, e = polygon
        for t in (1 / 3, 2 / 3):
            d.line([lerp(a, b, t), lerp(e, c, t)], fill=line, width=7)
            d.line([lerp(a, e, t), lerp(b, c, t)], fill=line, width=7)
        d.line([a, c], fill=line, width=7)
    for polygon, _ in faces:
        d.line(polygon + [polygon[0]], fill=(60, 66, 78, 255), width=12, joint="curve")

    big = img.resize((256, 256), Image.LANCZOS)
    big.save(HERE / "icon.png")
    big.save(HERE / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print("wrote icon.png and icon.ico")


if __name__ == "__main__":
    main()
