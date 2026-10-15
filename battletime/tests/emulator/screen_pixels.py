"""Reads one pixel of a screenshot an emulator saved, so a suite can tell which page the watch shows."""

import struct
import zlib

# Channels for each PNG colour type: grey, RGB, palette, grey with alpha, RGBA.
CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}


def _image(path):
    """The PNG's rows unfiltered, with its bit depth, colour type and palette; any depth, not interlaced."""
    with open(path, "rb") as file:
        data = file.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path} is not a PNG")
    position = 8
    compressed = b""
    palette = b""
    while position < len(data):
        length, kind = struct.unpack(">I4s", data[position:position + 8])
        body = data[position + 8:position + 8 + length]
        if kind == b"IHDR":
            width, height, depth, colour_type, _, _, interlace = struct.unpack(">IIBBBBB", body)
        elif kind == b"PLTE":
            palette = body
        elif kind == b"IDAT":
            compressed += body
        position += 12 + length
    if colour_type not in CHANNELS or interlace:
        raise ValueError(f"{path}: colour type {colour_type}, interlaced {interlace}, is not supported")
    pixel_bits = CHANNELS[colour_type] * depth
    # Filters work on whole bytes, a pixel's worth back, or one byte for pixels smaller than a byte.
    pixel_bytes = max(1, pixel_bits // 8)
    raw = zlib.decompress(compressed)
    stride = (width * pixel_bits + 7) // 8
    rows = []
    previous = bytearray(stride)
    for row_index in range(height):
        start = row_index * (stride + 1)
        filter_type = raw[start]
        row = bytearray(raw[start + 1:start + 1 + stride])
        for index in range(stride):
            left = row[index - pixel_bytes] if index >= pixel_bytes else 0
            up = previous[index]
            up_left = previous[index - pixel_bytes] if index >= pixel_bytes else 0
            if filter_type == 1:
                row[index] = (row[index] + left) & 0xFF
            elif filter_type == 2:
                row[index] = (row[index] + up) & 0xFF
            elif filter_type == 3:
                row[index] = (row[index] + (left + up) // 2) & 0xFF
            elif filter_type == 4:
                guess = left + up - up_left
                nearest = min((abs(guess - left), 0, left), (abs(guess - up), 1, up), (abs(guess - up_left), 2, up_left))
                row[index] = (row[index] + nearest[2]) & 0xFF
        rows.append(row)
        previous = row
    return rows, depth, colour_type, palette


def pixel_at(path, x, y):
    """The red, green and blue of one pixel, each 0 to 255."""
    rows, depth, colour_type, palette = _image(path)
    row = rows[int(y)]
    if depth < 8:
        bit = int(x) * depth
        shift = 8 - depth - bit % 8
        channels = [(row[bit // 8] >> shift) & ((1 << depth) - 1)]
        if colour_type == 0:
            channels = [channels[0] * 255 // ((1 << depth) - 1)]
    else:
        pixel_bytes = CHANNELS[colour_type] * depth // 8
        pixel = row[int(x) * pixel_bytes:(int(x) + 1) * pixel_bytes]
        # Sixteen-bit channels keep their high byte.
        channels = list(pixel[::2]) if depth == 16 else list(pixel)
    if colour_type == 3:
        return list(palette[channels[0] * 3:channels[0] * 3 + 3])
    if colour_type in (0, 4):
        return [channels[0]] * 3
    return channels[:3]
