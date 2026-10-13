"""Small images for the suites: a firmware that never confirms itself, and a bootloader that differs from the built one."""

import importlib.util
import struct
from pathlib import Path

_TOOL = Path(__file__).resolve().parents[2] / "tools" / "image_header.py"
_SPEC = importlib.util.spec_from_file_location("image_header", _TOOL)
image_header = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(image_header)

STACK_TOP = 0x2000FFF0
BRANCH_TO_SELF_TWICE = 0xE7FEE7FE


def write_never_confirming_image(path):
    """A headered firmware whose reset handler branches to itself: it starts, never confirms, and the watchdog resets."""
    load_address = image_header.FIRMWARE_VECTOR_TABLE
    code = struct.pack("<III", STACK_TOP, load_address + 8 + 1, BRANCH_TO_SELF_TWICE)
    header = image_header.header("firmware", "0.0.0", "", 0, code, load_address)
    Path(path).write_bytes(header + code)
    return str(path)


def write_older_bootloader(source, path):
    """The built bootloader with one byte of its title changed, so the updater sees a different bootloader to replace."""
    data = bytearray(Path(source).read_bytes())
    title = data.index(b"NeoTime bootloader")
    data[title + len("NeoTime ")] = ord("B")
    Path(path).write_bytes(bytes(data))
    return str(path)


def write_slot_table_record(path, update_start, update_size, recovery_start, recovery_size, sequence=1):
    """One slot table record as `neotime_boot::slot_table` writes it: magic, sequence, both slots, a spare word and a CRC."""
    import zlib

    numbers = [int(str(value), 0) for value in (sequence, update_start, update_size, recovery_start, recovery_size)]
    words = struct.pack("<4sIIIIII", b"NTST", *numbers, 0)
    Path(path).write_bytes(words + struct.pack("<I", zlib.crc32(words)))
    return str(path)


def write_damaged_copy(source, path, offset):
    """A copy of an image with one byte flipped, to spoil its header or its CRC."""
    data = bytearray(Path(source).read_bytes())
    data[int(offset)] ^= 0x01
    Path(path).write_bytes(bytes(data))
    return str(path)
