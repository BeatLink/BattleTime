#!/usr/bin/env bash
# Builds every image NeoTime ships and packs them into dist/: the bootloader, the firmware, and the
# two reloaders that move a watch over from wasp-os or InfiniTime. Run it inside `nix develop`.

set -euo pipefail
cd "$(dirname "$0")/.."

dist=dist
mkdir -p "$dist"

echo "bootloader"
(cd bootloader && cargo objcopy -q --release -- -O binary "../$dist/bootloader.bin")

echo "firmware"
(cd firmware && cargo objcopy -q --release -- -O binary "../$dist/firmware.bin")
python3 -I tools/package_dfu.py "$dist/firmware.bin" "$dist/firmware.zip"

# Until there is a smaller recovery firmware, the firmware itself serves as the recovery image.
export NEOTIME_BOOTLOADER_BIN="$PWD/$dist/bootloader.bin"
export NEOTIME_FIRMWARE_BIN="$PWD/$dist/firmware.bin"
export NEOTIME_RECOVERY_BIN="$PWD/$dist/firmware.bin"

echo "reloader from wasp-os"
(cd reloader && cargo objcopy -q --release --features from-wasp-os -- -O binary "../$dist/reloader-from-wasp-os.bin")
# wasp-os's bootloader on S112 (firmware ID 0xB8) is the only one that may install it.
python3 -I tools/package_dfu.py "$dist/reloader-from-wasp-os.bin" "$dist/reloader-from-wasp-os.zip" --sd-req 0xB8

echo "reloader from InfiniTime"
(cd reloader && cargo objcopy -q --release --features from-infinitime -- -O binary "../$dist/reloader-from-infinitime.bin")
python3 -I tools/mcuboot_image.py "$dist/reloader-from-infinitime.bin" "$dist/reloader-from-infinitime.mcuboot.bin"
# InfiniTime's own DFU takes the MCUBoot image in the same zip format.
python3 -I tools/package_dfu.py "$dist/reloader-from-infinitime.mcuboot.bin" "$dist/reloader-from-infinitime.zip"

ls -l "$dist"
