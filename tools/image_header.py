#!/usr/bin/env python3
"""Puts the 256-byte NeoTime image header in front of a firmware .bin, or prints the header of one.

The layout matches `neotime_boot::image_header`: magic "NTIM", header version and size, image type,
version, the address the image is linked to run from, its length and CRC-32, the build time, the
commit, and a CRC-32 of the header itself. The bootloader refuses to run or install an image whose
header is missing, damaged or linked for another address.
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
HEADER_VERSION = 1
FIRMWARE_VECTOR_TABLE = 0x8100
FIRMWARE_SLOT_BYTES = 388 * 1024
COMMIT_LENGTH = 8
IMAGE_TYPES = {"firmware": 1, "recovery": 2, "bootloader-updater": 3}
# magic, header version, header size, type, major, minor, patch, load address, size, CRC, build time, commit
FIELDS = struct.Struct("<4sHHBBBBIIII8s")
HEADER_CRC_OFFSET = HEADER_SIZE - 4


def header(image_type: str, version: str, commit: str, build_time: int, code: bytes, load_address: int) -> bytes:
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
    )
    body = fields.ljust(HEADER_CRC_OFFSET, b"\0")
    return body + struct.pack("<I", zlib.crc32(body))


def describe(image: bytes) -> str:
    if len(image) < HEADER_SIZE:
        return "too short for a header"
    body, (header_crc,) = image[:HEADER_CRC_OFFSET], struct.unpack("<I", image[HEADER_CRC_OFFSET:HEADER_SIZE])
    magic, header_version, header_size, type_number, major, minor, patch, load_address, size, crc, build_time, commit = (
        FIELDS.unpack(body[: FIELDS.size])
    )
    problems = []
    if magic != MAGIC or header_version != HEADER_VERSION or header_size != HEADER_SIZE:
        problems.append("not a NeoTime image header")
    if zlib.crc32(body) != header_crc:
        problems.append("header CRC differs")
    code = image[HEADER_SIZE:]
    if len(code) < size or zlib.crc32(code[:size]) != crc:
        problems.append("image CRC differs or image cut short")
    type_name = next((name for name, number in IMAGE_TYPES.items() if number == type_number), f"unknown {type_number}")
    return (
        f"{type_name} {major}.{minor}.{patch}+{commit.rstrip(b'\0').decode('ascii', 'replace')}, "
        f"{size} bytes linked at {load_address:#x}, built {time.strftime('%Y-%m-%d %H:%M:%S', time.gmtime(build_time))} UTC: "
        + ("; ".join(problems) if problems else "sound")
    )


def check_linked_for_firmware(code: bytes, load_address: int) -> None:
    stack_pointer, reset_handler = struct.unpack("<II", code[:8])
    if not 0x2000_0000 < stack_pointer <= 0x2001_0000:
        sys.exit(f"the image does not start with a stack pointer in RAM ({stack_pointer:#x})")
    if not load_address <= reset_handler & ~1 < load_address - HEADER_SIZE + FIRMWARE_SLOT_BYTES:
        sys.exit(f"the reset handler {reset_handler:#x} is not inside an image linked at {load_address:#x}")
    if HEADER_SIZE + len(code) > FIRMWARE_SLOT_BYTES:
        sys.exit(f"the image is {len(code)} bytes, more than the {FIRMWARE_SLOT_BYTES - HEADER_SIZE} the slot holds")


def self_test() -> None:
    test_header = header("recovery", "1.2.3", "abcdef0", 1_760_000_000, b"123456789", FIRMWARE_VECTOR_TABLE)
    assert len(test_header) == HEADER_SIZE
    assert test_header[:4] == MAGIC
    assert struct.unpack("<I", test_header[20:24])[0] == 0xCBF43926, "CRC-32 check value"
    # The same value boot/src/tests.rs expects, so this tool and the watch agree.
    assert struct.unpack("<I", test_header[HEADER_CRC_OFFSET:])[0] == 0x0479BFAA, hex(
        struct.unpack("<I", test_header[HEADER_CRC_OFFSET:])[0]
    )
    assert describe(test_header + b"123456789").endswith("sound")
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
    parser.add_argument("--describe", type=Path, help="print the header of an image and stop")
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()

    if arguments.self_test:
        self_test()
        return
    if arguments.describe:
        print(describe(arguments.describe.read_bytes()))
        return
    if arguments.image is None or arguments.output is None:
        parser.error("image and output are required")

    code = arguments.image.read_bytes()
    check_linked_for_firmware(code, arguments.load_address)
    build_time = arguments.build_time
    if build_time is None:
        build_time = int(os.environ.get("SOURCE_DATE_EPOCH", time.time()))
    image_header = header(arguments.type, arguments.version, arguments.commit, build_time, code, arguments.load_address)
    arguments.output.write_bytes(image_header + code)


if __name__ == "__main__":
    main()
