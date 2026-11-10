#!/usr/bin/env python3
"""Regenerate the website's optimized assets from their tracked sources.

Fonts: subset the Inter TTFs to the character ranges this site renders and
write WOFF2 next to them. Screenshots: write a native-width WebP plus a 640px
WebP for narrow screens next to each PNG.

Requires: fontTools, brotli, Pillow (pip install fonttools brotli Pillow).
Run from the repository root: python3 website/scripts/build-assets.py
"""
from pathlib import Path

from fontTools import subset
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
FONTS = ROOT / "assets-src/fonts"
SHOTS = ROOT / "public/screenshots"

# Basic Latin and Latin-1 (Umlaute, ·), Latin Extended-A, General Punctuation
# („ " – …) plus the arrow glyphs the page draws in text (↵ ↳ ↗).
UNICODES = "U+0020-007E,U+00A0-00FF,U+0100-017F,U+2000-206F,U+2190-21BB,U+20AC"
FEATURES = "kern,liga,calt,tnum,ss01"

for face in ("Inter-Regular", "Inter-SemiBold"):
    options = subset.Options()
    options.flavor = "woff2"
    options.layout_features = FEATURES.split(",")
    options.unicodes = UNICODES
    font = subset.load_font(str(FONTS / f"{face}.ttf"), options)
    subsetter = subset.Subsetter(options)
    def span(spec):
        spec = spec[2:]
        low, _, high = spec.partition("-")
        return range(int(low, 16), int(high or low, 16) + 1)
    codepoints = [c for spec in UNICODES.split(",") for c in span(spec)]
    subsetter.populate(unicodes=codepoints)
    subsetter.subset(font)
    subset.save_font(font, str(FONTS / f"{face}.latin.woff2"), options)
    print(f"subsets {face}.ttf -> {face}.latin.woff2")

for name in ("dark", "light", "dj-booth", "ai-studio"):
    image = Image.open(SHOTS / f"{name}.png").convert("RGB")
    image.save(SHOTS / f"{name}.webp", "WEBP", quality=83, method=6)
    narrow = image.resize((640, round(640 * image.height / image.width)), Image.LANCZOS)
    narrow.save(SHOTS / f"{name}-640.webp", "WEBP", quality=85, method=6)
    print(f"writes {name}.webp and {name}-640.webp")
