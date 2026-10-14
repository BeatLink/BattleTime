#!/usr/bin/env bash
# Builds every image NeoTime ships and packs them into dist/: the bootloader, the firmware with the
# shared library inside, the recovery firmware, the bootloader updaters, and the two reloaders that
# move a watch over from wasp-os or InfiniTime. Run it inside `nix develop`.

set -euo pipefail
cd "$(dirname "$0")/../neotime"

dist=dist
mkdir -p "$dist"

# Every image the NeoTime bootloader runs carries a header naming its type, version, commit and CRC.
version=$(sed -n 's/^version = "\(.*\)"/\1/p' firmware/Cargo.toml | head -1)
commit=$(git rev-parse --short=7 HEAD 2>/dev/null || echo unknown)
if ! git diff --quiet HEAD 2>/dev/null; then commit="$commit*"; fi
commit_time=$(git log -1 --format=%ct 2>/dev/null || echo 0)
add_header() { # type, code .bin, output .bin, then any further image_header.py options
  python3 -I ../tools/image_header.py --type "$1" --version "$version" --commit "$commit" --build-time "$commit_time" "$2" "$3" "${@:4}"
  rm "$2"
}

echo "bootloader"
(cd bootloader && cargo objcopy -q --release -- -O binary "../$dist/bootloader.bin")

echo "shared library"
cargo run -q --release -p neotime-compiler -- library -o "$dist/shared.ntlib"

echo "firmware, with the shared library after its code"
(cd firmware && cargo objcopy -q --release -- -O binary "../$dist/firmware.code.bin")
add_header firmware "$dist/firmware.code.bin" "$dist/firmware.bin" --library "$dist/shared.ntlib"
# Its symbols, which the emulator suites look up to poke statics; the later builds share its target folder.
cp firmware/target/thumbv7em-none-eabihf/release/neotime-firmware "$dist/firmware.elf"
python3 -I ../tools/package_dfu.py "$dist/firmware.bin" "$dist/firmware.zip"

echo "recovery firmware (the status page, Bluetooth and updates, without the shell or the library)"
(cd firmware && cargo objcopy -q --profile recovery --features recovery -- -O binary "../$dist/recovery.code.bin")
add_header recovery "$dist/recovery.code.bin" "$dist/recovery.bin"
python3 -I ../tools/package_dfu.py "$dist/recovery.bin" "$dist/recovery.zip"

echo "bootloader updater (a firmware update that installs dist/bootloader.bin on a watch already on NeoTime)"
(cd bootloader-updater && NEOTIME_NEW_BOOTLOADER_BIN="$PWD/../$dist/bootloader.bin" cargo objcopy -q --release -- -O binary "../$dist/bootloader-updater.code.bin")
add_header bootloader-updater "$dist/bootloader-updater.code.bin" "$dist/bootloader-updater.bin"
python3 -I ../tools/package_dfu.py "$dist/bootloader-updater.bin" "$dist/bootloader-updater.zip"

echo "bootloader updater from the first layout (16 KB bootloader, firmware at 0x5000; carries the recovery firmware)"
(cd bootloader-updater && NEOTIME_NEW_BOOTLOADER_BIN="$PWD/../$dist/bootloader.bin" NEOTIME_RECOVERY_BIN="$PWD/../$dist/recovery.bin" \
  CARGO_TARGET_DIR=target/from-first-layout cargo objcopy -q --release --features from-first-layout -- -O binary "../$dist/bootloader-updater-from-first-layout.bin")
python3 -I ../tools/package_dfu.py "$dist/bootloader-updater-from-first-layout.bin" "$dist/bootloader-updater-from-first-layout.zip"

echo "bootloader updater from the 388 KB layout (library at 0x69000; carries the recovery firmware)"
(cd bootloader-updater && NEOTIME_NEW_BOOTLOADER_BIN="$PWD/../$dist/bootloader.bin" NEOTIME_RECOVERY_BIN="$PWD/../$dist/recovery.bin" \
  CARGO_TARGET_DIR=target/from-388-kb-layout cargo objcopy -q --release --features from-388-kb-layout -- -O binary "../$dist/bootloader-updater-from-388-kb-layout.code.bin")
# The bootloader it is sent under reads only the header version of its own layout.
add_header bootloader-updater "$dist/bootloader-updater-from-388-kb-layout.code.bin" "$dist/bootloader-updater-from-388-kb-layout.bin" --for-388-kb-layout
python3 -I ../tools/package_dfu.py "$dist/bootloader-updater-from-388-kb-layout.bin" "$dist/bootloader-updater-from-388-kb-layout.zip"

# The full firmware does not fit beside the bootloader in wasp-os's app slot, so the reloader installs
# the recovery firmware in both places, and the full firmware follows as an ordinary update.
export NEOTIME_BOOTLOADER_BIN="$PWD/$dist/bootloader.bin"
export NEOTIME_FIRMWARE_BIN="$PWD/$dist/recovery.bin"
export NEOTIME_RECOVERY_BIN="$PWD/$dist/recovery.bin"

echo "reloader from wasp-os"
(cd reloader && cargo objcopy -q --release --features from-wasp-os -- -O binary "../$dist/reloader-from-wasp-os.bin")
# wasp-os's bootloader on S112 (firmware ID 0xB8) is the only one that may install it.
python3 -I ../tools/package_dfu.py "$dist/reloader-from-wasp-os.bin" "$dist/reloader-from-wasp-os.zip" --sd-req 0xB8

echo "reloader from InfiniTime"
(cd reloader && cargo objcopy -q --release --features from-infinitime -- -O binary "../$dist/reloader-from-infinitime.bin")
python3 -I ../tools/mcuboot_image.py "$dist/reloader-from-infinitime.bin" "$dist/reloader-from-infinitime.mcuboot.bin"
# InfiniTime's own DFU takes the MCUBoot image in the same zip format.
python3 -I ../tools/package_dfu.py "$dist/reloader-from-infinitime.mcuboot.bin" "$dist/reloader-from-infinitime.zip"

(cd "$dist" && sha256sum ./*.bin ./*.zip ./*.ntlib > SHA256SUMS)
ls -l "$dist"
