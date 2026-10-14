"""Turns the watch's font of lines, arcs and dots into the web fonts the browser preview draws text with.

Run it from anywhere after changing app-host/fonts/geometric.txt, then commit what it writes:

    nix-shell -p 'python3.withPackages (p: [ p.fonttools ])' --run 'python3 tools/generate_preview_font.py'

Browsers draw only filled outlines, so each shape becomes the outline of its stroke, with round ends:
a line becomes a capsule, an arc a band along it, and a dot a disc. Every outline runs clockwise and
a ring's hole anticlockwise, so where two strokes overlap they stay filled. It writes two weights into
sdk/neotime/src/preview_font.ts, as WOFF files in base64: the text sizes' and the light one the
huge and giant digits use. A CSS size of 16 px gives capitals 12 px tall, as the watch's small size.
"""

import base64
import io
import math
import sys
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

sys.path.insert(0, str(Path(__file__).resolve().parent))
import generate_fonts as source  # noqa: E402

OUTPUT = source.ROOT / "sdk" / "neotime" / "src" / "preview_font.ts"

# Font units are tenths of the source's, so curves stay smooth once rounded.
UNITS = 10
# Capitals are 100 source units; 16 px of CSS size should give capitals 12 px tall.
UNITS_PER_EM = round(100 * UNITS * 16 / 12)
ASCENT = 1300
DESCENT = 380

# The line's width against the capital height, as the watch draws its text sizes and its big digits.
WEIGHTS = [
    ("TEXT_FONT_WOFF", "NeoTime", 0.18),
    ("LIGHT_FONT_WOFF", "NeoTime Light", 0.08),
]
DOT_GROWTH = 1.5
# Points along each half turn of a round end or a whole circle.
POINTS_PER_HALF_TURN = 16


# ============================================================================
# Outlines of strokes
# ============================================================================


def clockwise(points):
    """The points in clockwise order, as TrueType wants outer contours."""
    area = sum(x1 * y2 - x2 * y1 for (x1, y1), (x2, y2) in zip(points, points[1:] + points[:1]))
    return points if area < 0 else points[::-1]


def half_turn(centre, radius, start_angle):
    """Points round half a circle anticlockwise from `start_angle`, the end included."""
    cx, cy = centre
    return [
        (cx + radius * math.cos(start_angle + math.pi * step / POINTS_PER_HALF_TURN),
         cy + radius * math.sin(start_angle + math.pi * step / POINTS_PER_HALF_TURN))
        for step in range(POINTS_PER_HALF_TURN + 1)
    ]


def disc(centre, radius):
    cx, cy = centre
    steps = 2 * POINTS_PER_HALF_TURN
    return [(cx + radius * math.cos(2 * math.pi * step / steps), cy + radius * math.sin(2 * math.pi * step / steps))
            for step in range(steps)]


def capsule(start, end, radius):
    """A line's stroke: its two sides joined by round ends."""
    (x1, y1), (x2, y2) = start, end
    if (x1, y1) == (x2, y2):
        return [clockwise(disc(start, radius))]
    angle = math.atan2(y2 - y1, x2 - x1)
    # Round the end, from the left side to the right, then round the start back.
    return [clockwise(half_turn(end, radius, angle - math.pi / 2) + half_turn(start, radius, angle + math.pi / 2))]


def band(centre, across, down, start, end, radius):
    """An arc's stroke: a whole ring as two contours, or a band with round ends."""
    cx, cy = centre
    sweep = (end - start) % 360 or 360
    steps = max(8, round(sweep / 4))

    def along(degrees, offset):
        t = math.radians(degrees)
        x, y = cx + across * math.cos(t), cy + down * math.sin(t)
        normal_x, normal_y = down * math.cos(t), across * math.sin(t)
        length = math.hypot(normal_x, normal_y) or 1
        return x + offset * normal_x / length, y + offset * normal_y / length

    angles = [start + sweep * step / steps for step in range(steps + 1)]
    if sweep == 360:
        outer = [along(angle, radius) for angle in angles[:-1]]
        inner = [along(angle, -radius) for angle in angles[:-1]]
        return [clockwise(outer), clockwise(inner)[::-1]]
    outer = [along(angle, radius) for angle in angles]
    inner = [along(angle, -radius) for angle in reversed(angles)]

    def end_cap(degrees, outward):
        t = math.radians(degrees)
        tangent = math.atan2(down * math.cos(t), -across * math.sin(t))
        point = along(degrees, 0)
        return half_turn(point, radius, tangent + (-math.pi / 2 if outward else math.pi / 2))

    return [clockwise(outer + end_cap(angles[-1], True)[1:-1] + inner + end_cap(angles[0], False)[1:-1])]


def contours_of(shapes, stroke):
    radius = stroke / 2
    contours = []
    for tag, numbers in shapes:
        scaled = [number * UNITS for number in numbers]
        if tag == source.LINE:
            x1, y1, x2, y2 = scaled
            contours += capsule((x1, y1), (x2, y2), radius)
        elif tag == source.ARC:
            x, y, across, down = scaled[:4]
            start, end = numbers[4], numbers[5]
            contours += band((x, y), across, down, start, end, radius)
        else:
            contours.append(clockwise(disc((scaled[0], scaled[1]), radius * DOT_GROWTH)))
    return contours


# ============================================================================
# Writing the fonts
# ============================================================================


def glyph_name(character):
    return f"uni{ord(character):04X}"


def build_font(glyphs, family, weight):
    stroke = weight * 100 * UNITS
    order = [".notdef"] + [glyph_name(character) for character in sorted(glyphs)]
    built, metrics = {}, {}
    empty = TTGlyphPen(None)
    built[".notdef"] = empty.glyph()
    metrics[".notdef"] = (source.STANDARD_WIDTH * UNITS, 0)
    for character in sorted(glyphs):
        width, shapes = glyphs[character]
        pen = TTGlyphPen(None)
        lowest_x = 0
        for contour in contours_of(shapes, stroke):
            points = [(round(x), round(y)) for x, y in contour]
            if len(points) < 3:
                continue
            pen.moveTo(points[0])
            for point in points[1:]:
                pen.lineTo(point)
            pen.closePath()
            lowest_x = min(lowest_x, *(x for x, _ in points))
        built[glyph_name(character)] = pen.glyph()
        metrics[glyph_name(character)] = (width * UNITS, lowest_x)
    builder = FontBuilder(UNITS_PER_EM, isTTF=True)
    builder.setupGlyphOrder(order)
    builder.setupCharacterMap({ord(character): glyph_name(character) for character in glyphs})
    builder.setupGlyf(built)
    for name, glyph in built.items():
        glyph.recalcBounds(builder.font["glyf"])
        if hasattr(glyph, "xMin"):
            metrics[name] = (metrics[name][0], glyph.xMin)
    builder.setupHorizontalMetrics(metrics)
    builder.setupHorizontalHeader(ascent=ASCENT, descent=-DESCENT)
    builder.setupNameTable({"familyName": family, "styleName": "Regular"})
    builder.setupOS2(sTypoAscender=ASCENT, sTypoDescender=-DESCENT, usWinAscent=ASCENT, usWinDescent=DESCENT)
    builder.setupPost()
    builder.font.flavor = "woff"
    file = io.BytesIO()
    builder.save(file)
    return file.getvalue()


def main():
    glyphs = source.read_glyphs(source.SOURCE)
    lines = [
        "// Generated by tools/generate_preview_font.py from app-host/fonts/geometric.txt; do not edit.",
        "// The watch's font of lines and arcs as WOFF web fonts, so the browser preview draws the same letters.",
        "",
    ]
    for constant, family, weight in WEIGHTS:
        woff = build_font(glyphs, family, weight)
        print(f"{family}: {len(woff)} bytes")
        lines.append(f'export const {constant} = "{base64.b64encode(woff).decode()}";')
    OUTPUT.write_text("\n".join(lines) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
