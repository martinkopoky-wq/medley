#!/usr/bin/env python3
"""Generate Medley's logo SVGs and app-icon PNGs.

    python3 -m venv .venv && .venv/bin/pip install cairosvg pillow
    .venv/bin/python assets/render_icons.py

Writes assets/logo.svg, assets/logo-mark.svg and assets/icon-{1024,512,256}.png.
logo.svg uses SVG filters for its soft shadows (browsers render them); the PNGs
are composited here with Pillow so they look the same without filter support.

The mark: three strands, one per service. Each rises on its own leg, arches
over, and all three weave together into a single point in the middle of the
"M" before parting again. Three streams, braided into one.
"""

from __future__ import annotations

import io
from pathlib import Path

import cairosvg
from PIL import Image, ImageFilter

ASSETS = Path(__file__).resolve().parent
CANVAS = 1024
# macOS app-icon grid: an 824 pt continuous-corner square centred on a 1024 pt canvas.
TILE_X, TILE_SIZE = 100, 824
TILE_RADIUS = TILE_SIZE * 0.2237

# Strands, innermost first (later strands weave over earlier ones):
# (left foot x, top of the arch y, cap radius). The right half mirrors the left.
STRANDS = [(380, 434, 54), (316, 354, 90), (252, 274, 126)]
STROKE = 38
GAP = 11  # knockout around a strand where it passes over another
FOOT_Y = 744
VALLEY = (512, 622)


def squircle(x: float, y: float, size: float, r: float) -> str:
    """Continuous-curvature rounded square (the iOS/macOS corner) as an SVG path."""
    a, b, c, d = 1.52866483, 1.08849323, 0.86840689, 0.63149399
    e, f, g = 0.07491100, 0.37282392, 0.16905899
    x0, y0, x1, y1 = x, y, x + size, y + size

    def p(px: float, py: float) -> str:
        return f"{px:.1f} {py:.1f}"

    return " ".join(
        [
            f"M{p(x0 + a * r, y0)}",
            f"L{p(x1 - a * r, y0)}",
            f"C{p(x1 - b * r, y0)} {p(x1 - c * r, y0)} {p(x1 - d * r, y0 + e * r)}",
            f"C{p(x1 - f * r, y0 + g * r)} {p(x1 - g * r, y0 + f * r)} {p(x1 - e * r, y0 + d * r)}",
            f"C{p(x1, y0 + c * r)} {p(x1, y0 + b * r)} {p(x1, y0 + a * r)}",
            f"L{p(x1, y1 - a * r)}",
            f"C{p(x1, y1 - b * r)} {p(x1, y1 - c * r)} {p(x1 - e * r, y1 - d * r)}",
            f"C{p(x1 - g * r, y1 - f * r)} {p(x1 - f * r, y1 - g * r)} {p(x1 - d * r, y1 - e * r)}",
            f"C{p(x1 - c * r, y1)} {p(x1 - b * r, y1)} {p(x1 - a * r, y1)}",
            f"L{p(x0 + a * r, y1)}",
            f"C{p(x0 + b * r, y1)} {p(x0 + c * r, y1)} {p(x0 + d * r, y1 - e * r)}",
            f"C{p(x0 + f * r, y1 - g * r)} {p(x0 + g * r, y1 - f * r)} {p(x0 + e * r, y1 - d * r)}",
            f"C{p(x0, y1 - c * r)} {p(x0, y1 - b * r)} {p(x0, y1 - a * r)}",
            f"L{p(x0, y0 + a * r)}",
            f"C{p(x0, y0 + b * r)} {p(x0, y0 + c * r)} {p(x0 + e * r, y0 + d * r)}",
            f"C{p(x0 + g * r, y0 + f * r)} {p(x0 + f * r, y0 + g * r)} {p(x0 + d * r, y0 + e * r)}",
            f"C{p(x0 + c * r, y0)} {p(x0 + b * r, y0)} {p(x0 + a * r, y0)}",
            "Z",
        ]
    )


def strand(lx: float, top: float, r: float) -> str:
    """Foot → straight leg → round cap → down into the shared valley, then mirrored."""
    k = 0.5523  # cubic approximation of a quarter circle
    vx, vy = VALLEY
    cx, rx, rcx = lx + r, CANVAS - lx, CANVAS - (lx + r)
    return (
        f"M{lx} {FOOT_Y} L{lx} {top + r} "
        f"C{lx} {top + r - k * r:.1f} {cx - k * r:.1f} {top} {cx} {top} "
        f"C{cx + k * r * 1.1:.1f} {top} {vx - 18} {vy - 42} {vx} {vy} "
        f"C{vx + 18} {vy - 42} {rcx - k * r * 1.1:.1f} {top} {rcx} {top} "
        f"C{rcx + k * r:.1f} {top} {rx} {top + r - k * r:.1f} {rx} {top + r} "
        f"L{rx} {FOOT_Y}"
    )


def glyph_defs(prefix: str) -> str:
    """One mask per strand, cutting a gap wherever a later strand passes over it."""
    masks = []
    for i in range(len(STRANDS) - 1):
        cuts = "".join(f'<path d="{strand(*s)}"/>' for s in STRANDS[i + 1 :])
        masks.append(
            f'<mask id="{prefix}{i}" maskUnits="userSpaceOnUse" x="0" y="0" width="{CANVAS}" height="{CANVAS}">'
            f'<rect width="{CANVAS}" height="{CANVAS}" fill="#fff"/>'
            f'<g fill="none" stroke="#000" stroke-width="{STROKE + 2 * GAP}" stroke-linecap="round" '
            f'stroke-linejoin="round">{cuts}</g></mask>'
        )
    return "\n  ".join(masks)


def glyph(color: str, prefix: str = "weave", extra: str = "") -> str:
    paths = []
    for i, s in enumerate(STRANDS):
        mask = f' mask="url(#{prefix}{i})"' if i < len(STRANDS) - 1 else ""
        paths.append(f'<path d="{strand(*s)}"{mask}/>')
    return (
        f'<g fill="none" stroke="{color}" stroke-width="{STROKE}" stroke-linecap="round" '
        f'stroke-linejoin="round"{extra}>' + "".join(paths) + "</g>"
    )


TILE = squircle(TILE_X, TILE_X, TILE_SIZE, TILE_RADIUS)


def defs(filters: str) -> str:
    return f"""<defs>
  <linearGradient id="bg" x1="200" y1="100" x2="824" y2="924" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="#FF9F0A"/>
    <stop offset="0.48" stop-color="#FF2D55"/>
    <stop offset="1" stop-color="#5E5CE6"/>
  </linearGradient>
  <linearGradient id="sheen" x1="0" y1="100" x2="0" y2="924" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="#FFFFFF" stop-opacity="0.22"/>
    <stop offset="0.5" stop-color="#FFFFFF" stop-opacity="0"/>
    <stop offset="1" stop-color="#000000" stop-opacity="0.10"/>
  </linearGradient>
  <linearGradient id="rim" x1="0" y1="100" x2="0" y2="924" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="#FFFFFF" stop-opacity="0.45"/>
    <stop offset="0.3" stop-color="#FFFFFF" stop-opacity="0"/>
  </linearGradient>
  <clipPath id="tile"><path d="{TILE}"/></clipPath>
  {glyph_defs("weave")}
  {glyph_defs("lift")}
  {filters}
</defs>"""


SVG_FILTERS = """<filter id="drop" x="-10%" y="-10%" width="120%" height="125%">
    <feGaussianBlur in="SourceAlpha" stdDeviation="14"/>
    <feOffset dy="12"/>
    <feComponentTransfer><feFuncA type="linear" slope="0.32"/></feComponentTransfer>
  </filter>
  <filter id="soft" x="-20%" y="-20%" width="140%" height="150%">
    <feGaussianBlur stdDeviation="10"/>
  </filter>"""


def tile_layers() -> str:
    return (
        f'<path d="{TILE}" fill="url(#bg)"/>'
        f'<path d="{TILE}" fill="url(#sheen)"/>'
        f'<path d="{TILE}" fill="none" stroke="url(#rim)" stroke-width="4" clip-path="url(#tile)"/>'
    )


def write_logo_svg() -> None:
    body = "\n".join(
        [
            f'<path d="{TILE}" fill="#000" filter="url(#drop)"/>',
            tile_layers(),
            '<g clip-path="url(#tile)"><g filter="url(#soft)" opacity="0.22" transform="translate(0 10)">'
            + glyph("#3A0A2A", prefix="lift")
            + "</g></g>",
            glyph("#FFFFFF"),
        ]
    )
    svg = (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">\n'
        "<title>Medley</title>\n" + defs(SVG_FILTERS) + "\n" + body + "\n</svg>\n"
    )
    (ASSETS / "logo.svg").write_text(svg)


def write_mark_svg() -> None:
    pad = STROKE / 2 + 6
    outer_x, outer_top, _ = STRANDS[-1]
    x0, x1 = outer_x - pad, CANVAS - outer_x + pad
    y0, y1 = outer_top - pad, FOOT_Y + pad
    w, h = x1 - x0, y1 - y0
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x0:.0f} {y0:.0f} {w:.0f} {h:.0f}" '
        f'width="{w / 4:.0f}" height="{h / 4:.0f}">\n'
        "<title>Medley</title>\n<defs>\n  " + glyph_defs("weave") + "\n</defs>\n" + glyph("currentColor") + "\n</svg>\n"
    )
    (ASSETS / "logo-mark.svg").write_text(svg)


def render(svg_body: str) -> Image.Image:
    svg = (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">'
        + defs("")
        + svg_body
        + "</svg>"
    )
    png = cairosvg.svg2png(bytestring=svg.encode(), output_width=CANVAS, output_height=CANVAS)
    return Image.open(io.BytesIO(png)).convert("RGBA")


def shadow(layer: Image.Image, blur: float, dy: int, color: tuple[int, int, int], opacity: float) -> Image.Image:
    alpha = layer.getchannel("A").point(lambda a: int(a * opacity))
    alpha = alpha.filter(ImageFilter.GaussianBlur(blur))
    tinted = Image.new("RGBA", layer.size, color + (0,))
    tinted.putalpha(alpha)
    shifted = Image.new("RGBA", layer.size, (0, 0, 0, 0))
    shifted.paste(tinted, (0, dy))
    return shifted


def write_pngs() -> None:
    tile = render(tile_layers())
    mark = render(glyph("#FFFFFF"))

    icon = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    icon.alpha_composite(shadow(tile, 14, 12, (0, 0, 0), 0.32))
    icon.alpha_composite(tile)
    lift = shadow(mark, 10, 10, (0x3A, 0x0A, 0x2A), 0.22)
    lift.putalpha(Image.composite(lift.getchannel("A"), Image.new("L", lift.size, 0), tile.getchannel("A")))
    icon.alpha_composite(lift)
    icon.alpha_composite(mark)

    for size in (1024, 512, 256):
        out = icon if size == CANVAS else icon.resize((size, size), Image.Resampling.LANCZOS)
        out.save(ASSETS / f"icon-{size}.png", optimize=True)


if __name__ == "__main__":
    write_logo_svg()
    write_mark_svg()
    write_pngs()
    print("wrote logo.svg, logo-mark.svg, icon-1024.png, icon-512.png, icon-256.png")
