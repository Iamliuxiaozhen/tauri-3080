#!/usr/bin/env python3
"""Render the 127.0.0.1:3080 app icon.

This script is the reproducible source for `assets/app-icon.png`. It only needs
Pillow and a monospace font (DejaVu Sans Mono ships with most Linux images and
is what the released icons were generated with).

Usage:
    python3 assets/render-icon.py [--size 1024] [--out assets/app-icon.png]
"""

from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# Palette shared with the frontend (see src/styles/base.css).
BACKDROP_TOP = (11, 24, 39)  # #0b1827
BACKDROP_BOTTOM = (16, 42, 67)  # #102a43
ACCENT = (94, 234, 212)  # #5eead4
FOREGROUND = (240, 253, 250)  # #f0fdfa
BADGE = (251, 191, 36)  # #fbbf24

FONT_CANDIDATES = (
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansMono-Bold.ttf",
    "/System/Library/Fonts/Menlo.ttc",
    "C:/Windows/Fonts/consola.ttf",
)


def find_font(size: float) -> ImageFont.FreeTypeFont:
    for candidate in FONT_CANDIDATES:
        if Path(candidate).exists():
            return ImageFont.truetype(candidate, size)
    raise SystemExit("No monospace font found; install DejaVu Sans Mono and retry.")


def vertical_gradient(size: int, top: tuple[int, int, int], bottom: tuple[int, int, int]) -> Image.Image:
    gradient = Image.new("RGB", (1, size))
    pixels = gradient.load()
    for y in range(size):
        blend = y / max(size - 1, 1)
        pixels[0, y] = tuple(round(top[i] + (bottom[i] - top[i]) * blend) for i in range(3))
    return gradient.resize((size, size), Image.Resampling.BICUBIC)


def render(size: int = 1024) -> Image.Image:
    scale = size / 1024
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    # Rounded square plate, 72% corner radius like the macOS icon grid.
    plate = Image.new("L", (size, size), 0)
    ImageDraw.Draw(plate).rounded_rectangle(
        (0, 0, size - 1, size - 1), radius=round(232 * scale), fill=255
    )
    plate_image = vertical_gradient(size, BACKDROP_TOP, BACKDROP_BOTTOM).convert("RGBA")
    canvas.paste(plate_image, (0, 0), plate)

    draw = ImageDraw.Draw(canvas)

    # Faint terminal frame: a rounded outline inset from the plate edge.
    inset = round(112 * scale)
    frame = (inset, inset, size - inset, size - inset)
    draw.rounded_rectangle(
        frame, radius=round(120 * scale), outline=(*ACCENT, 70), width=max(round(6 * scale), 2)
    )

    # The port itself, big and centered.
    text = "3080"
    font = find_font(round(352 * scale))
    left, top, right, bottom = draw.textbbox((0, 0), text, font=font)
    text_width = right - left
    text_height = bottom - top
    draw.text(
        ((size - text_width) / 2 - left, (size - text_height) / 2 - top - round(26 * scale)),
        text,
        font=font,
        fill=FOREGROUND,
    )

    # Listening indicator: an accent bar under the number.
    bar_width = round(196 * scale)
    bar_height = round(22 * scale)
    bar_y = (size + text_height) / 2 + round(56 * scale)
    draw.rounded_rectangle(
        ((size - bar_width) / 2, bar_y, (size + bar_width) / 2, bar_y + bar_height),
        radius=bar_height / 2,
        fill=ACCENT,
    )

    # Amber "live" dot in the top-right corner.
    dot_radius = round(46 * scale)
    dot_center = (size - round(168 * scale), round(168 * scale))
    draw.ellipse(
        (
            dot_center[0] - dot_radius,
            dot_center[1] - dot_radius,
            dot_center[0] + dot_radius,
            dot_center[1] + dot_radius,
        ),
        fill=BADGE,
    )

    return canvas


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", type=int, default=1024)
    parser.add_argument("--out", type=Path, default=Path("assets/app-icon.png"))
    args = parser.parse_args()

    icon = render(args.size)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    icon.save(args.out)
    print(f"wrote {args.out} ({args.size}x{args.size})")


if __name__ == "__main__":
    main()
