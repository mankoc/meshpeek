"""Generate MeshPeek's coloured toolbar icons (assets/toolbar/*.png, 96 px) with Pillow.

    python make_toolbar_icons.py
"""

import math
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent / "toolbar"
S = 384  # draw at 4x, downsample to 96
AMBER, AMBER_D = (244, 183, 64), (214, 140, 30)
TEAL, TEAL_D, TEAL_L = (70, 170, 200), (40, 120, 150), (130, 215, 235)
RED, RED_D = (226, 84, 84), (170, 50, 50)
PAPER, EDGE = (236, 240, 246), (96, 108, 126)


def canvas():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    return img, ImageDraw.Draw(img)


def save(img, name):
    OUT.mkdir(exist_ok=True)
    img.resize((96, 96), Image.LANCZOS).save(OUT / f"{name}.png")


def gem(d, cx, cy, r):
    """Small teal faceted hexagon, the application's motif."""
    outer = [(cx + r * math.cos(math.radians(-90 + 60 * i)), cy + r * math.sin(math.radians(-90 + 60 * i))) for i in range(6)]
    mid = (cx - r * 0.1, cy - r * 0.1)
    for i, k in zip(range(6), (1.2, 1.0, 0.75, 0.6, 0.85, 1.1)):
        d.polygon([outer[i], outer[(i + 1) % 6], mid], fill=tuple(min(255, int(v * k)) for v in TEAL) + (255,))
    d.line(outer + [outer[0]], fill=TEAL_D + (255,), width=10, joint="curve")


def open_file():
    img, d = canvas()
    d.polygon([(90, 30), (230, 30), (300, 100), (300, 354), (90, 354)], fill=PAPER + (255,), outline=EDGE + (255,), width=12)
    d.polygon([(230, 30), (230, 100), (300, 100)], fill=(190, 200, 214, 255), outline=EDGE + (255,), width=12)
    gem(d, 195, 235, 70)
    save(img, "open")


def open_folder():
    img, d = canvas()
    d.rounded_rectangle((30, 90, 354, 320), radius=26, fill=AMBER_D + (255,))
    d.polygon([(30, 90), (30, 70), (150, 70), (180, 100), (30, 100)], fill=AMBER_D + (255,))
    d.rounded_rectangle((30, 140, 354, 330), radius=26, fill=AMBER + (255,))
    gem(d, 260, 250, 48)
    save(img, "folder")


def clear():
    img, d = canvas()
    d.rounded_rectangle((60, 70, 324, 108), radius=14, fill=RED_D + (255,))
    d.rounded_rectangle((140, 36, 244, 78), radius=14, fill=RED_D + (255,))
    d.polygon([(84, 120), (300, 120), (278, 340), (106, 340)], fill=RED + (255,))
    for x in (150, 192, 234):
        d.line([(x, 150), (x - 4 if x < 192 else x + 4 if x > 192 else x, 312)], fill=(255, 220, 220, 255), width=14)
    save(img, "clear")


def objects():
    img, d = canvas()
    for i, (dy, c) in enumerate(((150, TEAL_D), (100, TEAL), (50, TEAL_L))):
        d.polygon([(192, 40 + dy), (340, 110 + dy), (192, 180 + dy), (44, 110 + dy)], fill=c + (255,), outline=(30, 70, 90, 255), width=10)
    save(img, "objects")


def eye():
    img, d = canvas()
    cx, cy, w, h = 192, 192, 165, 105
    up = [(cx + x, cy - h * (1 - (x / w) ** 2) ** 0.9) for x in [-w + 2 * w * i / 24 for i in range(25)]]
    lo = [(cx + x, cy + h * (1 - (x / w) ** 2) ** 0.9) for x in [w - 2 * w * i / 24 for i in range(25)]]
    d.polygon(up + lo, fill=PAPER + (255,))
    d.ellipse((cx - 78, cy - 78, cx + 78, cy + 78), fill=AMBER + (255,), outline=AMBER_D + (255,), width=10)
    d.ellipse((cx - 30, cy - 30, cx + 30, cy + 30), fill=(30, 32, 38, 255))
    d.ellipse((cx - 50, cy - 56, cx - 24, cy - 30), fill=(255, 255, 255, 255))
    d.line(up + [lo[0]], fill=EDGE + (255,), width=12, joint="curve")
    d.line(lo + [up[0]], fill=EDGE + (255,), width=12, joint="curve")
    save(img, "eye")


def colours():
    img, d = canvas()
    d.ellipse((30, 40, 354, 340), fill=(214, 178, 128, 255), outline=(150, 112, 70, 255), width=12)
    for (x, y), c in zip(((100, 130), (190, 95), (280, 130), (105, 235)), ((230, 70, 70), (250, 200, 50), (80, 180, 90), (70, 130, 230))):
        d.ellipse((x - 36, y - 36, x + 36, y + 36), fill=c + (255,), outline=(0, 0, 0, 60), width=4)
    d.ellipse((235, 215, 335, 315), fill=(0, 0, 0, 0))  # thumb hole
    d.ellipse((235, 215, 335, 315), outline=(150, 112, 70, 255), width=10)
    save(img, "colours")


if __name__ == "__main__":
    for make in (open_file, open_folder, clear, objects, eye, colours):
        make()
    print("wrote", OUT)
