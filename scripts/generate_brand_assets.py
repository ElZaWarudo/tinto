#!/usr/bin/env python3
"""Generate Tinto brand assets from brand/wordmark.png.

Crop boxes are pinned to the provided 1448x1086 brand sheet. The source mark is
the Balanced A recommendation, with the preferred light/dark presentations used
only to derive UI-ready transparent wordmarks.
"""

from __future__ import annotations

from math import sqrt
from pathlib import Path

from PIL import Image, ImageOps


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "brand" / "wordmark.png"
WEB_ASSETS = ROOT / "src" / "assets" / "brand"
PUBLIC_ASSETS = ROOT / "public"
TAURI_ICONS = ROOT / "src-tauri" / "icons"

# Balanced A: section 2, variant A, wordmark only.
CROP_BALANCED_A_WORDMARK = (565, 266, 704, 321)
# Preferred logo, light background: section 3, espresso wordmark.
CROP_PREFERRED_LIGHT_WORDMARK = (65, 610, 330, 705)
# Preferred logo, dark background reversed: section 3, white wordmark.
CROP_PREFERRED_DARK_WORDMARK = (390, 610, 680, 705)
# Small-scale behavior, app icon: section 4, square cup tile.
CROP_APP_ICON = (795, 580, 950, 735)

PNG_ICON_SIZES = {
    "32x32.png": 32,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "icon.png": 512,
    "Square30x30Logo.png": 30,
    "Square44x44Logo.png": 44,
    "Square71x71Logo.png": 71,
    "Square89x89Logo.png": 89,
    "Square107x107Logo.png": 107,
    "Square142x142Logo.png": 142,
    "Square150x150Logo.png": 150,
    "Square284x284Logo.png": 284,
    "Square310x310Logo.png": 310,
    "StoreLogo.png": 50,
}


def source_image() -> Image.Image:
    return Image.open(SOURCE).convert("RGBA")


def transparent_from_corners(
    image: Image.Image,
    *,
    tolerance: int = 22,
    feather: int = 14,
    padding: tuple[int, int, int, int] = (6, 4, 6, 4),
) -> Image.Image:
    """Remove the local sheet/card background using the crop corner color."""

    rgb = image.convert("RGB")
    corners = [
        rgb.getpixel((0, 0)),
        rgb.getpixel((rgb.width - 1, 0)),
        rgb.getpixel((0, rgb.height - 1)),
        rgb.getpixel((rgb.width - 1, rgb.height - 1)),
    ]
    bg = tuple(sum(pixel[channel] for pixel in corners) // len(corners) for channel in range(3))

    alpha = Image.new("L", rgb.size)
    rgb_pixels = rgb.load()
    alpha_pixels = alpha.load()
    for y in range(rgb.height):
        for x in range(rgb.width):
            distance = sqrt(sum((rgb_pixels[x, y][channel] - bg[channel]) ** 2 for channel in range(3)))
            alpha_pixels[x, y] = max(0, min(255, int((distance - tolerance) / feather * 255)))

    output = image.copy()
    output.putalpha(alpha)
    bbox = alpha.point(lambda value: 255 if value > 8 else 0).getbbox()
    if bbox:
        output = output.crop(bbox)

    return ImageOps.expand(output, border=padding, fill=(0, 0, 0, 0))


def resize_square(image: Image.Image, size: int) -> Image.Image:
    return image.resize((size, size), Image.Resampling.LANCZOS)


def save_png(image: Image.Image, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, optimize=True)


def main() -> None:
    sheet = source_image()
    light_wordmark = transparent_from_corners(sheet.crop(CROP_PREFERRED_LIGHT_WORDMARK))
    dark_wordmark = transparent_from_corners(sheet.crop(CROP_PREFERRED_DARK_WORDMARK))
    balanced_wordmark = transparent_from_corners(sheet.crop(CROP_BALANCED_A_WORDMARK))
    app_icon = sheet.crop(CROP_APP_ICON)

    save_png(dark_wordmark, WEB_ASSETS / "tinto-wordmark-dark.png")
    save_png(light_wordmark, WEB_ASSETS / "tinto-wordmark-light.png")
    save_png(balanced_wordmark, WEB_ASSETS / "tinto-wordmark-balanced-a.png")
    save_png(resize_square(app_icon, 512), WEB_ASSETS / "tinto-icon.png")

    save_png(resize_square(app_icon, 256), PUBLIC_ASSETS / "tinto-icon.png")
    save_png(resize_square(app_icon, 32), PUBLIC_ASSETS / "favicon.png")

    for filename, size in PNG_ICON_SIZES.items():
        save_png(resize_square(app_icon, size), TAURI_ICONS / filename)

    icon_512 = resize_square(app_icon, 512)
    icon_512.save(TAURI_ICONS / "icon.ico", sizes=[(32, 32), (64, 64), (128, 128), (256, 256)])
    icon_512.save(TAURI_ICONS / "icon.icns")


if __name__ == "__main__":
    main()
