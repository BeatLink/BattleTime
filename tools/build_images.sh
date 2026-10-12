#!/usr/bin/env bash
# Builds every image NeoTime ships and packs them into dist/: the bootloader, the firmware, the
# recovery firmware, the bootloader updater, and the two reloaders that move a watch over from wasp-os or InfiniTime. Run it
# inside `nix develop`.

set -euo pipefail
cd "$(dirname "$0")/.."

dist=dist
mkdir -p "$dist"

echo "bootloader"
(cd bootloader && cargo objcopy -q --release -- -O binary "../$dist/bootloader.bin")

echo "firmware"
(cd firmware && cargo objcopy -q --release -- -O binary "../$dist/firmware.bin")
python3 -I tools/package_dfu.py "$dist/firmware.bin" "$dist/firmware.zip"

echo "recovery firmware (the status page, Bluetooth and updates, without the shell)"
(cd firmware && cargo objcopy -q --release --features recovery -- -O binary "../$dist/recovery.bin")
python3 -I tools/package_dfu.py "$dist/recovery.bin" "$dist/recovery.zip"

echo "rehearsal (an app for wasp-os that writes no flash)"
(cd firmware && cargo objcopy -q --release --features wasp-os-rehearsal -- -O binary "../$dist/rehearsal-under-wasp-os.bin")
python3 -I tools/package_dfu.py "$dist/rehearsal-under-wasp-os.bin" "$dist/rehearsal-under-wasp-os.zip" --sd-req 0xB8
echo "storage rehearsal (also erases wasp-os's files on the external flash)"
(cd firmware && cargo objcopy -q --release --features wasp-os-storage-rehearsal -- -O binary "../$dist/storage-rehearsal-under-wasp-os.bin")
python3 -I tools/package_dfu.py "$dist/storage-rehearsal-under-wasp-os.bin" "$dist/storage-rehearsal-under-wasp-os.zip" --sd-req 0xB8
# A 16 KB upload for the rehearsal's update service, which keeps it in RAM and checks it.
python3 -I -c "import random; random.seed(1); open('$dist/rehearsal-upload.bin', 'wb').write(bytes(random.getrandbits(8) for _ in range(16384)))"
python3 -I tools/package_dfu.py "$dist/rehearsal-upload.bin" "$dist/rehearsal-upload.zip"

echo "bootloader updater (a firmware update that installs dist/bootloader.bin on a watch already on NeoTime)"
(cd bootloader-updater && NEOTIME_NEW_BOOTLOADER_BIN="$PWD/../$dist/bootloader.bin" cargo objcopy -q --release -- -O binary "../$dist/bootloader-updater.bin")
python3 -I tools/package_dfu.py "$dist/bootloader-updater.bin" "$dist/bootloader-updater.zip"

# The full firmware does not fit beside the bootloader in wasp-os's app slot, so the reloader installs
# the recovery firmware in both places, and the full firmware follows as an ordinary update.
export NEOTIME_BOOTLOADER_BIN="$PWD/$dist/bootloader.bin"
export NEOTIME_FIRMWARE_BIN="$PWD/$dist/recovery.bin"
export NEOTIME_RECOVERY_BIN="$PWD/$dist/recovery.bin"

echo "reloader from wasp-os"
(cd reloader && cargo objcopy -q --release --features from-wasp-os -- -O binary "../$dist/reloader-from-wasp-os.bin")
# wasp-os's bootloader on S112 (firmware ID 0xB8) is the only one that may install it.
python3 -I tools/package_dfu.py "$dist/reloader-from-wasp-os.bin" "$dist/reloader-from-wasp-os.zip" --sd-req 0xB8

echo "reloader from InfiniTime"
(cd reloader && cargo objcopy -q --release --features from-infinitime -- -O binary "../$dist/reloader-from-infinitime.bin")
python3 -I tools/mcuboot_image.py "$dist/reloader-from-infinitime.bin" "$dist/reloader-from-infinitime.mcuboot.bin"
# InfiniTime's own DFU takes the MCUBoot image in the same zip format.
python3 -I tools/package_dfu.py "$dist/reloader-from-infinitime.mcuboot.bin" "$dist/reloader-from-infinitime.zip"

(cd "$dist" && sha256sum ./*.bin ./*.zip > SHA256SUMS)
ls -l "$dist"
