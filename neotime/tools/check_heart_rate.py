"""Checks the heart rate sensor end to end in the PineTime emulator: the firmware reads the pulse the emulated HRS3300 gives.

It runs two watches. On the first, with a face, the launcher and the heart app installed, it
opens the heart app from the launcher, sets the emulated pulse, and reads the firmware's
diagnostics counters for the sensor's state and its latest reading; it then changes the pulse and
checks the reading follows. The second has the heart rate interval setting on always and no app
open, so the reading must come from background measuring alone.

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
FIRMWARE_VECTOR_TABLE = 0x8100
IMAGE_HEADER_SIZE = 256
# The UICR words embassy-nrf writes on the first start, so the emulated chip is not reset.
FIRST_START_UICR = [(0x10001200, 0x15), (0x10001204, 0x15), (0x1000120C, 0xFFFFFFFE)]
COUNTERS_SYMBOL = "neotime_firmware::diagnostics::COUNTERS"
# The heart rate counters are the last two of diagnostics.rs: the sensor's state, then the latest reading.
STARTED = 1
FIRST_PULSE = 84
SECOND_PULSE = 120
BACKGROUND_PULSE = 66
# One window of samples takes about eight seconds, and later readings come every four.
FIRST_READING_SECONDS = 12
NEXT_READING_SECONDS = 10
LARGEST_ERROR = 2
# `/settings/system` with one record: the format byte, then setting 12 (the heart rate interval) at 1, always.
ALWAYS_MEASURING_SETTINGS = bytes([1, 12]) + struct.pack("<i", 1)


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


def compile_install(folder):
    """Compiles the packages and the shared library; gives the packages as ID=FILE pairs and the library."""
    run(["cargo", "build", "-q", "-p", "neotime-compiler"])
    compiler = str(REPOSITORY / "target" / "debug" / "neotime-compiler")
    pairs = []
    for package_id, source in PACKAGES:
        image = folder / f"{package_id}.ntapp"
        run([compiler, source, "-o", str(image)])
        pairs.append(f"{package_id}={image}")
    library = folder / "library.bin"
    run([compiler, "library", "-o", str(library)])
    return pairs, library


def library_lines(library, folder):
    """Emulator lines that load the image header and the library behind the ELF, where the firmware finds it."""
    header = folder / "header.bin"
    address = run([sys.executable, "-I", str(REPOSITORY / "tools" / "image_header.py"),
                   "--library-beside-elf", str(library), str(header)]).strip()
    return [
        f"sysbus LoadBinary @{header} {FIRMWARE_VECTOR_TABLE - IMAGE_HEADER_SIZE:#x}",
        f"sysbus LoadBinary @{library} {address}",
    ]


def write_flash(path, entries):
    """Writes the external flash image holding these ID=FILE packages and /PATH=FILE files."""
    run(["cargo", "run", "-q", "-p", "neotime-packages", "--example", "filesystem_use", "--",
         "--no-theme", "--image", str(path), *entries])
    return path


# ============================================================================
# Running it
# ============================================================================

def run_watch(emulator, firmware, library, flash, steps, folder):
    """Starts the firmware on this install, runs the steps, and gives the emulator's output."""
    lines = [f"include @{emulator}/scripts/pinetime.resc"]
    lines += [f"sysbus WriteDoubleWord {address:#x} {value:#x}" for address, value in FIRST_START_UICR]
    lines += [
        f"sysbus LoadELF @{firmware}",
        *library_lines(library, folder),
        f"externalFlashMemory LoadBinary @{flash} 0",
        f"cpu VectorTableOffset {FIRMWARE_VECTOR_TABLE:#x}",
        f"cpu SP `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE:#x}`",
        f"cpu PC `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE + 4:#x}`",
    ]
    lines += steps + ["quit"]
    script = folder / "heart_rate.resc"
    script.write_text("\n".join(lines) + "\n")
    engine = subprocess.run(
        ["nix", "develop", str(emulator), "-c", "pinetime-engine", str(script)],
        capture_output=True, text=True, cwd=emulator, stdin=subprocess.DEVNULL, timeout=900,
    )
    return engine.stdout + engine.stderr


def readings(output, counters_size, expected):
    """The sensor's state and reading from each read of the counters, or exits with what the emulator said."""
    read = bytes(int(byte, 16) for byte in re.findall(r"0x([0-9A-Fa-f]{2})\b", output))
    if len(read) < counters_size * expected:
        sys.exit("the emulator gave too few readings:\n" + output[-4000:])
    words = [struct.unpack_from(f"<{counters_size // 4}I", read, index * counters_size) for index in range(expected)]
    return [(counters[-2], counters[-1]) for counters in words]


def check(name, pulse, reading):
    state, beats_per_minute = reading
    passed = state == STARTED and abs(beats_per_minute - pulse) <= LARGEST_ERROR
    print(f"{'ok  ' if passed else 'FAIL'}  {name}, pulse {pulse}: sensor state {state}, reading {beats_per_minute}")
    return passed


def main():
    firmware = Path(sys.argv[1]).resolve()
    screenshot = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
    counters, counters_size = symbol(firmware, COUNTERS_SYMBOL)
    read_counters = f"sysbus ReadBytes {counters:#x} {counters_size:#x}"
    emulator = emulator_folder()

    with tempfile.TemporaryDirectory() as temporary:
        folder = Path(temporary)
        pairs, library = compile_install(folder)

        app_steps = [
            f"sysbus.twi1.heartRateSensor SetHeartRate {FIRST_PULSE}",
            'emulation RunFor "00:00:05"',
            'sysbus.serial1.touch Swipe "up"',
            'emulation RunFor "00:00:03"',
            "sysbus.serial1.touch Tap 120 40",
            f'emulation RunFor "00:00:{FIRST_READING_SECONDS:02}"',
            read_counters,
            f"sysbus.twi1.heartRateSensor SetHeartRate {SECOND_PULSE}",
            f'emulation RunFor "00:00:{NEXT_READING_SECONDS:02}"',
            read_counters,
        ]
        if screenshot:
            app_steps.append(f"lcd SaveScreenshot @{screenshot}")
        flash = write_flash(folder / "app.bin", pairs)
        app_readings = readings(run_watch(emulator, firmware, library, flash, app_steps, folder), counters_size, 2)

        settings = folder / "settings"
        settings.write_bytes(ALWAYS_MEASURING_SETTINGS)
        background_steps = [
            f"sysbus.twi1.heartRateSensor SetHeartRate {BACKGROUND_PULSE}",
            f'emulation RunFor "00:00:{5 + FIRST_READING_SECONDS:02}"',
            read_counters,
        ]
        flash = write_flash(folder / "background.bin", [*pairs, f"/settings/system={settings}"])
        background_reading = readings(
            run_watch(emulator, firmware, library, flash, background_steps, folder), counters_size, 1
        )

    results = [
        check("heart app", FIRST_PULSE, app_readings[0]),
        check("heart app", SECOND_PULSE, app_readings[1]),
        check("background, no app open", BACKGROUND_PULSE, background_reading[0]),
    ]
    sys.exit(0 if all(results) else 1)


main()
