"""Turns the font drawn from lines, arcs and dots in app-host/fonts/geometric.txt into the file the app host reads.

Run it from anywhere after changing a glyph, then tools/generate_preview_font.py for the browser preview:

    python3 tools/generate_fonts.py

It writes app-host/fonts/geometric.ntfont, which app-host/src/fonts.rs reads in place:

    header, 16 bytes: magic "NTGF", version (1), reserved (1), glyph count (2), capital height (1),
                      x-height (1), standard width (1), descender (1, signed), then the lowest,
                      highest, leftmost and rightmost any shape's centre line reaches (1 each, signed)
    one 6-byte entry per glyph, sorted by character: character (2), width (1), shape count (1),
                      offset of its first shape from the end of the table (2)
    shapes, each a kind byte and then signed coordinates in font units:
        0 line: x1, y1, x2, y2
        1 arc:  centre x, centre y, radius across (unsigned), radius down (unsigned),
                start and end angle, anticlockwise, in 256ths of a turn; equal angles draw it whole
        2 dot:  x, y

Every size the app host draws (small to giant) is this one font, scaled.
"""

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "battletime"
SOURCE = ROOT / "app-host" / "fonts" / "geometric.txt"
OUTPUT = ROOT / "app-host" / "fonts" / "geometric.ntfont"

CAPITAL_HEIGHT = 100
X_HEIGHT = 76
STANDARD_WIDTH = 82
DESCENDER = -26

# The app host keeps a glyph's placed shapes on the stack while it draws, so a glyph may have only this many.
MOST_SHAPES = 8

LINE, ARC, DOT = 0, 1, 2
SHAPE_FIELDS = {"L": (LINE, 4), "A": (ARC, 6), "D": (DOT, 2)}


# ============================================================================
# Reading the source
# ============================================================================


def character_of(token):
    if token.startswith("U+") and len(token) > 2:
        return chr(int(token[2:], 16))
    if len(token) != 1:
        raise ValueError(f"{token!r} is neither one character nor U+XXXX")
    return token


def moved(shape, across, up):
    """A shape moved `across` and `up` font units."""
    tag, numbers = shape
    if tag == ARC:
        return tag, [numbers[0] + across, numbers[1] + up] + numbers[2:]
    return tag, [number + (across if index % 2 == 0 else up) for index, number in enumerate(numbers)]


def parse_shape(text, glyphs, where):
    """One shape, or every shape of an earlier glyph for @c, moved by @c dx dy."""
    if text.startswith("@"):
        name, *offset = text[1:].split()
        named = character_of(name)
        if named not in glyphs:
            raise ValueError(f"{where}: @{name} names a glyph not drawn yet")
        if len(offset) not in (0, 2):
            raise ValueError(f"{where}: @{name} takes no offset or two numbers")
        across, up = (int(number) for number in offset) if offset else (0, 0)
        return [moved(shape, across, up) for shape in glyphs[named][1]]
    kind, *numbers = text.split()
    if kind not in SHAPE_FIELDS:
        raise ValueError(f"{where}: unknown shape {kind!r}")
    tag, count = SHAPE_FIELDS[kind]
    if len(numbers) != count:
        raise ValueError(f"{where}: {kind} takes {count} numbers, not {len(numbers)}")
    return [(tag, [int(number) for number in numbers])]


def read_glyphs(path):
    """Every glyph in the source, as character -> (width, shapes)."""
    glyphs = {}
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = line.strip()
        if not line or line.startswith("//"):
            continue
        where = f"{path.name}:{number}"
        key, _, rest = line.partition(" ")
        character = character_of(key)
        rest = rest.strip()
        width = STANDARD_WIDTH
        first, _, after = rest.partition(" ")
        if first.isdigit():
            width, rest = int(first), after.strip()
        shapes = []
        for part in filter(None, (piece.strip() for piece in rest.split(";"))):
            shapes += parse_shape(part, glyphs, where)
        if character in glyphs:
            raise ValueError(f"{where}: {character!r} is drawn twice")
        if len(shapes) > MOST_SHAPES:
            raise ValueError(f"{where}: {character!r} has {len(shapes)} shapes, more than {MOST_SHAPES}")
        glyphs[character] = (width, shapes)
    return glyphs


# ============================================================================
# Writing the font
# ============================================================================


def angle_steps(degrees):
    return round(degrees % 360 * 256 / 360) % 256


def signed_byte(value, what):
    if not -128 <= value <= 127:
        raise ValueError(f"{what} {value} does not fit a byte")
    return value & 0xFF


def unsigned_byte(value, what):
    if not 0 <= value <= 255:
        raise ValueError(f"{what} {value} does not fit a byte")
    return value


def encode_shape(tag, numbers, character):
    what = f"{character!r}:"
    if tag == ARC:
        centre_x, centre_y, across, down, start, end = numbers
        start_steps, end_steps = angle_steps(start), angle_steps(end)
        if start_steps == end_steps and start % 360 != end % 360:
            raise ValueError(f"{what} an arc from {start} to {end} is too short to keep")
        return bytes([
            tag,
            signed_byte(centre_x, what),
            signed_byte(centre_y, what),
            unsigned_byte(across, what),
            unsigned_byte(down, what),
            start_steps,
            end_steps,
        ])
    return bytes([tag] + [signed_byte(number, what) for number in numbers])


def reach(shapes):
    """Every x and every y a glyph's centre lines reach, a whole arc's box counted for an arc."""
    xs, ys = [], []
    for tag, numbers in shapes:
        if tag == LINE:
            xs += [numbers[0], numbers[2]]
            ys += [numbers[1], numbers[3]]
        elif tag == ARC:
            xs += [numbers[0] - numbers[2], numbers[0] + numbers[2]]
            ys += [numbers[1] - numbers[3], numbers[1] + numbers[3]]
        else:
            xs.append(numbers[0])
            ys.append(numbers[1])
    return xs, ys


def build_font(glyphs):
    table = bytearray()
    data = bytearray()
    for character in sorted(glyphs):
        width, shapes = glyphs[character]
        table += ord(character).to_bytes(2, "little")
        table += bytes([unsigned_byte(width, f"{character!r} width"), len(shapes)])
        table += len(data).to_bytes(2, "little")
        for tag, numbers in shapes:
            data += encode_shape(tag, numbers, character)

    header = bytearray(b"NTGF")
    header += bytes([1, 0])
    header += len(glyphs).to_bytes(2, "little")
    xs = [x for _, shapes in glyphs.values() for x in reach(shapes)[0]]
    ys = [y for _, shapes in glyphs.values() for y in reach(shapes)[1]]
    header += bytes([CAPITAL_HEIGHT, X_HEIGHT, STANDARD_WIDTH, DESCENDER & 0xFF])
    header += bytes(signed_byte(value, "reach") for value in (min(ys), max(ys), min(xs), max(xs)))
    return bytes(header + table + data)


def main():
    glyphs = read_glyphs(SOURCE)
    font = build_font(glyphs)
    OUTPUT.write_bytes(font)
    print(f"{OUTPUT.name}: {len(glyphs)} glyphs, {len(font)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
