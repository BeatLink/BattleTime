#!/usr/bin/env python3
"""Packs a firmware .bin into the zip that legacy DFU clients send: manifest, init packet and image.

The init packet carries a CRC-16 of the image, which the watch checks before installing. With
--sd-req the zip is only accepted by a bootloader running that SoftDevice; wasp-os's S112
bootloader is 0xB8, and NeoTime ignores the field.
"""

import argparse
import json
import struct
import zipfile
from pathlib import Path

DEVICE_TYPE_NRF52 = 0x0052
ANY_REVISION = 0xFFFF
ANY_VERSION = 0xFFFFFFFF
ANY_SOFTDEVICE = 0xFFFE


def crc16(data: bytes, crc: int = 0xFFFF) -> int:
    """CRC-16/CCITT-FALSE, as Nordic's tools and bootloaders compute it."""
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) if crc & 0x8000 else (crc << 1)
            crc &= 0xFFFF
    return crc


def init_packet(image: bytes, softdevice_requirements: list[int]) -> bytes:
    header = struct.pack("<HHIH", DEVICE_TYPE_NRF52, ANY_REVISION, ANY_VERSION, len(softdevice_requirements))
    requirements = struct.pack("<" + "H" * len(softdevice_requirements), *softdevice_requirements)
    return header + requirements + struct.pack("<H", crc16(image))


def write_package(image_path: Path, output_path: Path, softdevice_requirements: list[int]) -> None:
    image = image_path.read_bytes()
    stem = image_path.stem
    manifest = {
        "manifest": {
            "application": {
                "bin_file": f"{stem}.bin",
                "dat_file": f"{stem}.dat",
                "init_packet_data": {
                    "application_version": ANY_VERSION,
                    "device_revision": ANY_REVISION,
                    "device_type": DEVICE_TYPE_NRF52,
                    "firmware_crc16": crc16(image),
                    "softdevice_req": softdevice_requirements,
                },
            }
        }
    }
    with zipfile.ZipFile(output_path, "w", zipfile.ZIP_DEFLATED) as package:
        package.writestr("manifest.json", json.dumps(manifest, indent=2))
        package.writestr(f"{stem}.dat", init_packet(image, softdevice_requirements))
        package.writestr(f"{stem}.bin", image)


def self_test() -> None:
    assert crc16(b"123456789") == 0x29B1, "CRC-16 check value"
    packet = init_packet(b"123456789", [0xB8])
    assert packet == bytes.fromhex("5200ffffffffffff0100b800b129"), packet.hex()
    print("ok")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("image", nargs="?", type=Path, help="the firmware .bin")
    parser.add_argument("output", nargs="?", type=Path, help="the zip to write")
    parser.add_argument("--sd-req", type=lambda text: int(text, 0), action="append", help="a SoftDevice firmware ID the bootloader must have; repeatable")
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()

    if arguments.self_test:
        self_test()
        return
    if arguments.image is None or arguments.output is None:
        parser.error("image and output are required")
    write_package(arguments.image, arguments.output, arguments.sd_req or [ANY_SOFTDEVICE])


if __name__ == "__main__":
    main()
