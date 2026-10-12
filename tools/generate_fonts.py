"""Turns the free fonts in third_party/fonts into the compact bitmap fonts the app host draws with.

Run it from the repository root after changing a font below:

    nix-shell -p 'python3.withPackages (p: [ p.pillow p.fonttools ])' --run 'python3 tools/generate_fonts.py'

Each font becomes app-host/fonts/<name>.ntfont, which app-host/src/fonts.rs reads in place:

    header, 16 bytes: magic "NTFN", version (1), bits per pixel (4), glyph count (2),
                      line height (2), ascent (2), descent (2), reserved (2)
    one 10-byte entry per glyph, sorted by character: character (2), data offset (3),
                      advance (1), left bearing (1, signed), top bearing above the baseline
                      (1, signed), width (1), height (1)
    glyph data: 4-bit coverage, rows top to bottom, two pixels a byte with the first in the high
                half; a 0 or a 15 is followed by a half byte holding how many more of it follow,
                so the empty and solid parts of large digits cost almost nothing.

The seven-segment and dot-matrix fonts have no data: app-host/src/fonts.rs draws them as shapes.
"""

import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
SOURCES = ROOT / "third_party" / "fonts"
OUTPUT = ROOT / "app-host" / "fonts"

# Characters the ports show beyond plain ASCII: degrees, the calculator's signs and keys, and the weather's ticks and crosses.
EXTRA_TEXT = "°×÷±←✓✕"
TEXT = "".join(chr(code) for code in range(0x20, 0x7F)) + EXTRA_TEXT
CLOCK = " 0123456789:-.+%°AMP"
DIGITS = " 0123456789:"

# Name, source file, size in pixels, characters, how much to squeeze it across.
FONTS = [
    ("small", "JetBrainsMono-Bold.ttf", 16, TEXT, 1.0),
    ("normal", "JetBrainsMono-Bold.ttf", 20, TEXT, 1.0),
    ("medium", "JetBrainsMono-Bold.ttf", 30, TEXT, 1.0),
    ("large", "JetBrainsMono-ExtraBold.ttf", 42, TEXT, 1.0),
    ("huge", "JetBrainsMono-Light.ttf", 76, CLOCK, 1.0),
    ("giant", "OpenSans-Light.ttf", 120, DIGITS, 1.0),
    ("condensed", "JetBrainsMono-Bold.ttf", 48, DIGITS + "-", 0.7),
]

# Fonts with only digits get a line just tall enough for them, plus this much room above and below.
DISPLAY_FONT_MARGIN = 2


# ============================================================================
# Rendering glyphs
# ============================================================================


def render_glyph(font, character, squeeze):
    """Returns (coverage image or None, left bearing, top bearing, advance) for one character."""
    advance = round(font.getlength(character) * squeeze)
    left, top, right, bottom = font.getbbox(character, anchor="ls")
    if right <= left or bottom <= top:
        return None, 0, 0, advance

    picture = Image.new("L", (right - left, bottom - top), 0)
    ImageDraw.Draw(picture).text((-left, -top), character, font=font, fill=255, anchor="ls")
    if squeeze != 1.0:
        squeezed_width = max(1, round(picture.width * squeeze))
        picture = picture.resize((squeezed_width, picture.height), Image.LANCZOS)
        left = round(left * squeeze)

    inked = picture.getbbox()
    if inked is None:
        return None, 0, 0, advance
    picture = picture.crop(inked)
    return picture, left + inked[0], -(top + inked[1]), advance


def to_four_bits(coverage):
    return (coverage * 15 + 127) // 255


def encode(picture):
    """The glyph's coverage as half bytes, with runs of 0 and 15 shortened."""
    values = [to_four_bits(value) for value in picture.tobytes()]
    halves = []
    index = 0
    while index < len(values):
        value = values[index]
        if value in (0, 15):
            run = 1
            while run < 16 and index + run < len(values) and values[index + run] == value:
                run += 1
            halves += [value, run - 1]
            index += run
        else:
            halves.append(value)
            index += 1
    if len(halves) % 2:
        halves.append(0)
    return bytes((halves[position] << 4) | halves[position + 1] for position in range(0, len(halves), 2))


# ============================================================================
# Writing a font
# ============================================================================


def build_font(source, size, characters, squeeze):
    path = SOURCES / source
    font = ImageFont.truetype(str(path), size)
    character_map = TTFont(str(path)).getBestCmap()
    ascent, descent = font.getmetrics()

    entries = []
    data = bytearray()
    tallest_above = 0
    deepest_below = 0
    for character in sorted(set(characters)):
        if ord(character) not in character_map:
            print(f"  {source} has no {character!r}, left out")
            continue
        picture, left, top, advance = render_glyph(font, character, squeeze)
        width = height = 0
        offset = len(data)
        if picture is not None:
            width, height = picture.size
            data += encode(picture)
            tallest_above = max(tallest_above, top)
            deepest_below = max(deepest_below, height - top)
        entries.append((ord(character), offset, advance, left, top, width, height))

    if characters is not TEXT:
        ascent = tallest_above + DISPLAY_FONT_MARGIN
        descent = deepest_below + DISPLAY_FONT_MARGIN

    header = bytearray(b"NTFN")
    header += bytes([1, 4])
    header += len(entries).to_bytes(2, "little")
    header += (ascent + descent).to_bytes(2, "little")
    header += ascent.to_bytes(2, "little", signed=True)
    header += descent.to_bytes(2, "little", signed=True)
    header += bytes(2)

    table = bytearray()
    for code, offset, advance, left, top, width, height in entries:
        table += code.to_bytes(2, "little")
        table += offset.to_bytes(3, "little")
        table += bytes([advance & 0xFF, left & 0xFF, top & 0xFF, width, height])
    return bytes(header + table + data)


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    total = 0
    for name, source, size, characters, squeeze in FONTS:
        font_bytes = build_font(source, size, characters, squeeze)
        (OUTPUT / f"{name}.ntfont").write_bytes(font_bytes)
        total += len(font_bytes)
        print(f"{name}: {len(font_bytes)} bytes")
    print(f"all fonts: {total} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
