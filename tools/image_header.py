#!/usr/bin/env python3
"""Puts the 256-byte BattleTime image header in front of a firmware .bin, or prints the header of one.

The layout matches `battletime_boot::image_header`: magic "NTIM", header version and size, image type,
version, the address the image is linked to run from, its length and CRC-32, the build time, the
commit, where the shared library sits, and a CRC-32 of the header itself. The bootloader refuses to
run or install an image whose header is missing, damaged or linked for another address.

`--library` appends a shared library after the code, on a word boundary, and records its place.
`--library-beside-elf` is for emulator runs that load the firmware's ELF: it writes a header that
places the library at the end of the firmware region, and prints the address to load the library at.
"""

import argparse
import os
import struct
import sys
import time
import zlib
from pathlib import Path

HEADER_SIZE = 256
MAGIC = b"NTIM"
HEADER_VERSION = 2
FIRMWARE_VECTOR_TABLE = 0x8100
FIRMWARE_SLOT_BYTES = 480 * 1024
LIBRARY_ALIGNMENT = 4
COMMIT_LENGTH = 8
IMAGE_TYPES = {"firmware": 1, "recovery": 2, "bootloader-updater": 3}
# magic, header version, header size, type, major, minor, patch, load address, size, CRC, build time, commit,
# library offset from the start of the header, library size
FIELDS = struct.Struct("<4sHHBBBBIIII8sII")
HEADER_CRC_OFFSET = HEADER_SIZE - 4


def header(
    image_type: str,
    version: str,
    commit: str,
    build_time: int,
    code: bytes,
    load_address: int,
    library_offset: int = 0,
    library_size: int = 0,
) -> bytes:
    """The header for `code`, which already holds the library, if any, at `library_offset` minus the header."""
    major, minor, patch = (int(part) for part in version.split("."))
    fields = FIELDS.pack(
        MAGIC,
        HEADER_VERSION,
        HEADER_SIZE,
        IMAGE_TYPES[image_type],
        major,
        minor,
        patch,
        load_address,
        len(code),
        zlib.crc32(code),
        build_time,
        commit.encode("ascii")[:COMMIT_LENGTH].ljust(COMMIT_LENGTH, b"\0"),
        library_offset,
        library_size,
    )
    body = fields.ljust(HEADER_CRC_OFFSET, b"\0")
    return body + struct.pack("<I", zlib.crc32(body))


def describe(image: bytes) -> str:
    if len(image) < HEADER_SIZE:
        return "too short for a header"
    body, (header_crc,) = image[:HEADER_CRC_OFFSET], struct.unpack("<I", image[HEADER_CRC_OFFSET:HEADER_SIZE])
    (
        magic,
        header_version,
        header_size,
        type_number,
        major,
        minor,
        patch,
        load_address,
        size,
        crc,
        build_time,
        commit,
        library_offset,
        library_size,
    ) = FIELDS.unpack(body[: FIELDS.size])
    problems = []
    if magic != MAGIC or header_size != HEADER_SIZE:
        problems.append("not a BattleTime image header")
    elif header_version != HEADER_VERSION:
        problems.append(f"unknown header version {header_version}")
    if zlib.crc32(body) != header_crc:
        problems.append("header CRC differs")
    code = image[HEADER_SIZE:]
    if len(code) < size or zlib.crc32(code[:size]) != crc:
        problems.append("image CRC differs or image cut short")
    if library_size and not HEADER_SIZE <= library_offset <= library_offset + library_size <= HEADER_SIZE + size:
        problems.append("library lies outside the image")
    type_name = next((name for name, number in IMAGE_TYPES.items() if number == type_number), f"unknown {type_number}")
    library = f", library of {library_size} bytes at +{library_offset:#x}" if library_size else ""
    return (
        f"{type_name} {major}.{minor}.{patch}+{commit.rstrip(b'\0').decode('ascii', 'replace')}, "
        f"{size} bytes linked at {load_address:#x}{library}, built {time.strftime('%Y-%m-%d %H:%M:%S', time.gmtime(build_time))} UTC: "
        + ("; ".join(problems) if problems else "sound")
    )


def check_linked_for_firmware(code: bytes, load_address: int, slot_bytes: int) -> None:
    stack_pointer, reset_handler = struct.unpack("<II", code[:8])
    if not 0x2000_0000 < stack_pointer <= 0x2001_0000:
        sys.exit(f"the image does not start with a stack pointer in RAM ({stack_pointer:#x})")
    if not load_address <= reset_handler & ~1 < load_address - HEADER_SIZE + slot_bytes:
        sys.exit(f"the reset handler {reset_handler:#x} is not inside an image linked at {load_address:#x}")
    if HEADER_SIZE + len(code) > slot_bytes:
        sys.exit(f"the image is {len(code)} bytes, more than the {slot_bytes - HEADER_SIZE} the slot holds")


def with_library(code: bytes, library: bytes) -> tuple[bytes, int]:
    """The code with the library after it on a word boundary, and the library's offset from the header's start."""
    padded = code + b"\xff" * (-len(code) % LIBRARY_ALIGNMENT)
    return padded + library, HEADER_SIZE + len(padded)


def library_beside_elf(library_size: int) -> tuple[bytes, int]:
    """A header placing a library of `library_size` bytes at the end of the firmware region, and that address.

    Its image CRC is left zero: only the bootloader checks it, and an emulator run that loads the ELF has none.
    """
    firmware_start = FIRMWARE_VECTOR_TABLE - HEADER_SIZE
    library_offset = (FIRMWARE_SLOT_BYTES - library_size) // LIBRARY_ALIGNMENT * LIBRARY_ALIGNMENT
    fields = FIELDS.pack(
        MAGIC, HEADER_VERSION, HEADER_SIZE, IMAGE_TYPES["firmware"], 0, 0, 0, FIRMWARE_VECTOR_TABLE,
        library_offset + library_size - HEADER_SIZE, 0, 0, b"emulator", library_offset, library_size,
    )
    body = fields.ljust(HEADER_CRC_OFFSET, b"\0")
    return body + struct.pack("<I", zlib.crc32(body)), firmware_start + library_offset


def self_test() -> None:
    test_header = header("recovery", "1.2.3", "abcdef0", 1_760_000_000, b"123456789", FIRMWARE_VECTOR_TABLE, 260, 5)
    assert len(test_header) == HEADER_SIZE
    assert test_header[:4] == MAGIC
    assert struct.unpack("<I", test_header[20:24])[0] == 0xCBF43926, "CRC-32 check value"
    # The same value boot/src/tests.rs expects, so this tool and the watch agree.
    assert struct.unpack("<I", test_header[HEADER_CRC_OFFSET:])[0] == 0x98DEB2F0, hex(
        struct.unpack("<I", test_header[HEADER_CRC_OFFSET:])[0]
    )
    assert describe(test_header + b"123456789").endswith("sound")
    code, offset = with_library(b"\x01\x02\x03", b"LIB")
    assert (code, offset) == (b"\x01\x02\x03\xffLIB", HEADER_SIZE + 4)
    outside = header("firmware", "1.2.3", "", 0, code, FIRMWARE_VECTOR_TABLE, offset, 4)
    assert describe(outside + code).endswith("library lies outside the image")
    beside, address = library_beside_elf(1001)
    assert address % LIBRARY_ALIGNMENT == 0 and address + 1001 <= FIRMWARE_VECTOR_TABLE - HEADER_SIZE + FIRMWARE_SLOT_BYTES
    assert struct.unpack("<II", beside[36:44]) == (address - FIRMWARE_VECTOR_TABLE + HEADER_SIZE, 1001)
    print("ok")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("image", nargs="?", type=Path, help="the .bin straight from objcopy")
    parser.add_argument("output", nargs="?", type=Path, help="the .bin to write, header first")
    parser.add_argument("--type", choices=IMAGE_TYPES, default="firmware")
    parser.add_argument("--version", default="0.0.0", help="major.minor.patch")
    parser.add_argument("--commit", default="", help="short commit hash, with * for uncommitted changes")
    parser.add_argument("--build-time", type=int, help="seconds since 1970; defaults to SOURCE_DATE_EPOCH or now")
    parser.add_argument("--load-address", type=lambda text: int(text, 0), default=FIRMWARE_VECTOR_TABLE)
    parser.add_argument("--library", type=Path, help="a compiled shared library to append after the code")
    parser.add_argument("--describe", type=Path, help="print the header of an image and stop")
    parser.add_argument(
        "--library-beside-elf",
        nargs=2,
        type=Path,
        metavar=("LIBRARY", "HEADER_OUT"),
        help="write a header for an emulator run from the ELF and print where to load the library",
    )
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()

    if arguments.self_test:
        self_test()
        return
    if arguments.describe:
        print(describe(arguments.describe.read_bytes()))
        return
    if arguments.library_beside_elf:
        library, header_out = arguments.library_beside_elf
        header_bytes, address = library_beside_elf(library.stat().st_size)
        header_out.write_bytes(header_bytes)
        print(f"{address:#x}")
        return
    if arguments.image is None or arguments.output is None:
        parser.error("image and output are required")

    code = arguments.image.read_bytes()
    library_offset = library_size = 0
    if arguments.library:
        library = arguments.library.read_bytes()
        code, library_offset = with_library(code, library)
        library_size = len(library)
    check_linked_for_firmware(code, arguments.load_address, FIRMWARE_SLOT_BYTES)
    build_time = arguments.build_time
    if build_time is None:
        build_time = int(os.environ.get("SOURCE_DATE_EPOCH", time.time()))
    image_header = header(
        arguments.type,
        arguments.version,
        arguments.commit,
        build_time,
        code,
        arguments.load_address,
        library_offset,
        library_size,
    )
    arguments.output.write_bytes(image_header + code)


if __name__ == "__main__":
    main()
