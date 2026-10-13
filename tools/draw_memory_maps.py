"""Draws the NeoTime memory maps in docs/memory-map.md as treemaps; run it after changing a size below."""

import sys
from pathlib import Path

WIDTH, HEIGHT, GAP = 800, 420, 3

COLORS = {
    "boot": "#3b6fb6",
    "firmware": "#2a9d8f",
    "data": "#5a9e4b",
    "runtime": "#c77d2e",
    "spare": "#8a8f98",
}

MEMORIES = {
    "internal-flash": (
        "Internal flash, 512 KB",
        [
            ("Firmware", "0x08000", 388, "firmware"),
            ("Shared library", "0x69000", 92, "data"),
            ("Bootloader", "0x00000", 20, "boot"),
            ("Slot table", "0x05000", 8, "boot"),
            ("Boot state", "0x07000", 4, "boot"),
        ],
    ),
    "external-flash": (
        "External SPI flash, 4 MB",
        [
            ("File system", "0x100000", 3072, "data"),
            ("Update slot", "0x000000", 392, "firmware"),
            ("Recovery slot", "0x065000", 388, "boot"),
            ("Free for slots", "0x0C6000", 232, "spare"),
            ("Free for slots", "0x062000", 12, "spare"),
        ],
    ),
    "ram": (
        "RAM, 64 KB (planned budget)",
        [
            ("UI heap", "", 20, "runtime"),
            ("Bluetooth", "", 12, "runtime"),
            ("Apps", "", 6, "runtime"),
            ("Stack", "", 8, "boot"),
            ("Tasks", "", 6, "boot"),
            ("Reserve", "", 3, "spare"),
            ("System state", "", 4, "data"),
            ("Display buffer", "", 2, "firmware"),
            ("Flash store", "", 3, "data"),
        ],
    ),
}


def worst_ratio(row_areas, side):
    total = sum(row_areas)
    return max(max(side * side * area / (total * total), (total * total) / (side * side * area)) for area in row_areas)


def squarify(areas, x, y, width, height):
    rectangles = []
    remaining = list(areas)
    while remaining:
        side = min(width, height)
        row = [remaining.pop(0)]
        while remaining and worst_ratio(row + [remaining[0]], side) <= worst_ratio(row, side):
            row.append(remaining.pop(0))
        row_total = sum(row)
        if width >= height:
            column_width = row_total / height
            offset = y
            for area in row:
                rectangles.append((x, offset, column_width, area / column_width))
                offset += area / column_width
            x += column_width
            width -= column_width
        else:
            row_height = row_total / width
            offset = x
            for area in row:
                rectangles.append((offset, y, area / row_height, row_height))
                offset += area / row_height
            y += row_height
            height -= row_height
    return rectangles


def size_text(kilobytes):
    if kilobytes >= 1024 and kilobytes % 1024 == 0:
        return f"{kilobytes // 1024} MB"
    return f"{kilobytes} KB"


def draw(title, regions):
    total = sum(region[2] for region in regions)
    title_height = 34
    scale = WIDTH * (HEIGHT - title_height) / total
    areas = [region[2] * scale for region in regions]
    rectangles = squarify(areas, 0, title_height, WIDTH, HEIGHT - title_height)

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {WIDTH} {HEIGHT}" '
        f'font-family="Helvetica, Arial, sans-serif">',
        f'<rect width="{WIDTH}" height="{HEIGHT}" rx="6" fill="#1f2328"/>',
        f'<text x="12" y="23" font-size="17" font-weight="bold" fill="#ffffff">{title}</text>',
    ]
    too_small_to_label = []
    for (name, address, kilobytes, kind), (x, y, width, height) in zip(regions, rectangles):
        x, y = x + GAP, y + GAP
        width, height = width - 2 * GAP, height - 2 * GAP
        parts.append(
            f'<rect x="{x:.1f}" y="{y:.1f}" width="{width:.1f}" height="{height:.1f}" rx="4" '
            f'fill="{COLORS[kind]}"/>'
        )
        lines = [name, size_text(kilobytes)] + ([address] if address else [])
        longest_line = max(len(line) for line in lines)
        font_size = min(16, width / (0.62 * longest_line + 1), height / (1.3 * len(lines) + 0.6))
        if font_size < 8:
            too_small_to_label.append(f"{name} {size_text(kilobytes)} {address}".strip())
            continue
        line_height = font_size * 1.25
        first_line_y = y + height / 2 - line_height * (len(lines) - 1) / 2 + font_size / 3
        for index, line in enumerate(lines):
            weight = "bold" if index == 0 else "normal"
            parts.append(
                f'<text x="{x + width / 2:.1f}" y="{first_line_y + index * line_height:.1f}" '
                f'font-size="{font_size:.1f}" font-weight="{weight}" fill="#ffffff" '
                f'text-anchor="middle">{line}</text>'
            )
    if too_small_to_label:
        note = "Too small to label: " + ", ".join(too_small_to_label)
        parts.append(
            f'<text x="{WIDTH - 12}" y="23" font-size="13" fill="#c9d1d9" text-anchor="end">{note}</text>'
        )
    parts.append("</svg>")
    return "\n".join(parts) + "\n"


def main():
    output_folder = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent.parent / "docs" / "images"
    output_folder.mkdir(parents=True, exist_ok=True)
    for file_name, (title, regions) in MEMORIES.items():
        (output_folder / f"{file_name}.svg").write_text(draw(title, regions))


main()
