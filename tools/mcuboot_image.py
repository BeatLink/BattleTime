#!/usr/bin/env python3
"""Wraps a .bin linked at 0x8020 as the MCUBoot image InfiniTime's bootloader expects: a 32-byte header in front and a SHA-256 after.

This is the part of MCUBoot's imgtool that `imgtool create --align 4 --version 1.0.0 --header-size 32 --pad-header` does,
without signing, which InfiniTime's bootloader does not check.
"""

import argparse
import hashlib
import struct
from pathlib import Path

IMAGE_MAGIC = 0x96F3B83D
HEADER_SIZE = 32
TLV_INFO_MAGIC = 0x6907
TLV_SHA256 = 0x10


def mcuboot_image(image: bytes, version: tuple[int, int, int, int]) -> bytes:
    major, minor, revision, build = version
    header = struct.pack(
        "<IIHHIIBBHII",
        IMAGE_MAGIC,
        0,  # load address: run in place
        HEADER_SIZE,
        0,  # no protected TLVs
        len(image),
        0,  # flags
        major,
        minor,
        revision,
        build,
        0,
    )
    assert len(header) == HEADER_SIZE

    digest = hashlib.sha256(header + image).digest()
    tlv_entry = struct.pack("<BBH", TLV_SHA256, 0, len(digest)) + digest
    tlv_info = struct.pack("<HH", TLV_INFO_MAGIC, 4 + len(tlv_entry))
    return header + image + tlv_info + tlv_entry


def parse_version(text: str) -> tuple[int, int, int, int]:
    parts = [int(part) for part in text.split(".")]
    while len(parts) < 4:
        parts.append(0)
    return tuple(parts[:4])


def self_test() -> None:
    wrapped = mcuboot_image(b"\x00" * 100, (1, 0, 0, 0))
    assert wrapped[:4] == struct.pack("<I", IMAGE_MAGIC)
    assert struct.unpack("<I", wrapped[12:16])[0] == 100
    assert struct.unpack("<HH", wrapped[132:136]) == (TLV_INFO_MAGIC, 40)
    assert wrapped[140:172] == hashlib.sha256(wrapped[:132]).digest()
    print("ok")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("image", nargs="?", type=Path, help="the .bin linked at 0x8020")
    parser.add_argument("output", nargs="?", type=Path, help="the MCUBoot image to write")
    parser.add_argument("--version", default="1.0.0", type=parse_version)
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()

    if arguments.self_test:
        self_test()
        return
    if arguments.image is None or arguments.output is None:
        parser.error("image and output are required")
    arguments.output.write_bytes(mcuboot_image(arguments.image.read_bytes(), arguments.version))


if __name__ == "__main__":
    main()
