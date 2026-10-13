"""Checks the heart rate sensor end to end in the PineTime emulator: the firmware reads the pulse the emulated HRS3300 gives.

It installs a face, the launcher and the heart app, opens the heart app from the launcher, sets
the emulated pulse, and reads the firmware's diagnostics counters for the sensor's state and its
latest reading. It then changes the pulse and checks the reading follows.

    python3 -I tools/check_heart_rate.py FIRMWARE_ELF [SCREENSHOT]

Run it inside `nix develop`, with the firmware built (`cd firmware && cargo build --release`). It
finds the emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator folder beside the main checkout.
"""

import os
import re
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parent.parent

# ============================================================================
# The install and the watch
# ============================================================================

PACKAGES = [("digital", "faces/digital/face.tsx"), ("launcher", "apps/launcher/app.tsx"), ("heart", "apps/heart/app.tsx")]
SHARED_LIBRARY_START = 0x69000
FIRMWARE_VECTOR_TABLE = 0x8100
# The UICR words embassy-nrf writes on the first start, so the emulated chip is not reset.
FIRST_START_UICR = [(0x10001200, 0x15), (0x10001204, 0x15), (0x1000120C, 0xFFFFFFFE)]
COUNTERS_SYMBOL = "neotime_firmware::diagnostics::COUNTERS"
# The heart rate counters are the last two of diagnostics.rs: the sensor's state, then the latest reading.
STARTED = 1
FIRST_PULSE = 84
SECOND_PULSE = 120
# One window of samples takes about eight seconds, and later readings come every four.
FIRST_READING_SECONDS = 12
NEXT_READING_SECONDS = 10
LARGEST_ERROR = 2


def run(command, folder=REPOSITORY):
    return subprocess.run(command, capture_output=True, text=True, cwd=folder, check=True).stdout


def symbol(firmware, name):
    """The address and size of a symbol in the firmware."""
    for line in run(["nm", "-S", "-C", str(firmware)]).splitlines():
        parts = line.split(None, 3)
        if len(parts) == 4 and parts[3] == name:
            return int(parts[0], 16), int(parts[1], 16)
    sys.exit(f"{firmware} has no {name}")


def emulator_folder():
    if "PINETIME_EMULATOR" in os.environ:
        return Path(os.environ["PINETIME_EMULATOR"])
    common = run(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"]).strip()
    return Path(common).parent.parent / "PineTimeEmulator"


def build_install(folder):
    """Compiles the packages and the shared library, and writes the external flash image; gives the two files."""
    run(["cargo", "build", "-q", "-p", "neotime-compiler"])
    compiler = str(REPOSITORY / "target" / "debug" / "neotime-compiler")
    pairs = []
    for package_id, source in PACKAGES:
        image = folder / f"{package_id}.ntapp"
        run([compiler, source, "-o", str(image)])
        pairs.append(f"{package_id}={image}")
    library = folder / "library.bin"
    run([compiler, "library", "-o", str(library)])
    # The library goes behind its `NTLB` header, as the firmware finds it in internal flash.
    installed_library = folder / "library-installed.bin"
    image = library.read_bytes()
    installed_library.write_bytes(b"NTLB" + struct.pack("<I", len(image)) + image)
    flash = folder / "flash.bin"
    run(["cargo", "run", "-q", "-p", "neotime-packages", "--example", "filesystem_use", "--",
         "--no-theme", "--image", str(flash), *pairs])
    return installed_library, flash


# ============================================================================
# Running it
# ============================================================================

def main():
    firmware = Path(sys.argv[1]).resolve()
    screenshot = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
    counters, counters_size = symbol(firmware, COUNTERS_SYMBOL)
    emulator = emulator_folder()

    with tempfile.TemporaryDirectory() as folder:
        library, flash = build_install(Path(folder))
        lines = [f"include @{emulator}/scripts/pinetime.resc"]
        lines += [f"sysbus WriteDoubleWord {address:#x} {value:#x}" for address, value in FIRST_START_UICR]
        lines += [
            f"sysbus LoadELF @{firmware}",
            f"sysbus LoadBinary @{library} {SHARED_LIBRARY_START:#x}",
            f"externalFlashMemory LoadBinary @{flash} 0",
            f"cpu VectorTableOffset {FIRMWARE_VECTOR_TABLE:#x}",
            f"cpu SP `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE:#x}`",
            f"cpu PC `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE + 4:#x}`",
            f"sysbus.twi1.heartRateSensor SetHeartRate {FIRST_PULSE}",
            'emulation RunFor "00:00:05"',
            'sysbus.serial1.touch Swipe "up"',
            'emulation RunFor "00:00:03"',
            "sysbus.serial1.touch Tap 120 40",
            f'emulation RunFor "00:00:{FIRST_READING_SECONDS:02}"',
            f"sysbus ReadBytes {counters:#x} {counters_size:#x}",
            f"sysbus.twi1.heartRateSensor SetHeartRate {SECOND_PULSE}",
            f'emulation RunFor "00:00:{NEXT_READING_SECONDS:02}"',
            f"sysbus ReadBytes {counters:#x} {counters_size:#x}",
        ]
        if screenshot:
            lines.append(f"lcd SaveScreenshot @{screenshot}")
        lines.append("quit")
        script = Path(folder) / "heart_rate.resc"
        script.write_text("\n".join(lines) + "\n")
        engine = subprocess.run(
            ["nix", "develop", str(emulator), "-c", "pinetime-engine", str(script)],
            capture_output=True, text=True, cwd=emulator, stdin=subprocess.DEVNULL, timeout=900,
        )

    read = bytes(int(byte, 16) for byte in re.findall(r"0x([0-9A-Fa-f]{2})\b", engine.stdout))
    if len(read) < counters_size * 2:
        sys.exit("the emulator gave too few readings:\n" + engine.stdout[-2000:] + engine.stderr[-2000:])
    failed = False
    for index, pulse in enumerate([FIRST_PULSE, SECOND_PULSE]):
        words = struct.unpack_from(f"<{counters_size // 4}I", read, index * counters_size)
        state, beats_per_minute = words[-2], words[-1]
        passed = state == STARTED and abs(beats_per_minute - pulse) <= LARGEST_ERROR
        failed |= not passed
        print(f"{'ok  ' if passed else 'FAIL'}  pulse {pulse}: sensor state {state}, reading {beats_per_minute}")
    sys.exit(1 if failed else 0)


main()
