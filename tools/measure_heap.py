"""Measures the firmware's heap in the PineTime emulator with the face, the launcher and the first app open.

It starts the firmware on an external flash image such as `filesystem_use --image` writes, swipes up
to the launcher, taps its first row and presses the button to go back. At each step it reads the
allocator's bytes in use and the highest heap byte that is not zero, and saves a screenshot.

    python3 -I tools/measure_heap.py FIRMWARE_ELF FLASH_IMAGE SHARED_LIBRARY [SCREENSHOT_FOLDER [START_SECONDS]]

START_SECONDS, 5 unless given, is how long the watch starts before the face is read; a watch with
many apps installed takes longer to read them all.

Run it inside `nix develop`. It finds the emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator folder beside the
main checkout; see
docs/memory-map.md for what the numbers mean.
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
# The firmware's heap
# ============================================================================

HEAP_SYMBOL = "battletime_firmware::HEAP"
HEAP_MEMORY_SYMBOL = "battletime_firmware::start_heap::HEAP_MEMORY"
# The allocator's bytes in use are its second word.
USED_WORD = 1
FIRMWARE_VECTOR_TABLE = 0x8100
IMAGE_HEADER_SIZE = 256
# The UICR words embassy-nrf writes on the first start, so the emulated chip is not reset.
FIRST_START_UICR = [(0x10001200, 0x15), (0x10001204, 0x15), (0x1000120C, 0xFFFFFFFE)]
START_SECONDS = 5
STEPS = [
    ("face", []),
    ("launcher", ['sysbus.serial1.touch Swipe "up"', 'emulation RunFor "00:00:03"']),
    ("app", ["sysbus.serial1.touch Tap 120 40", 'emulation RunFor "00:00:04"']),
    ("back", ["sysbus.serial1.sideButton PressAndRelease", 'emulation RunFor "00:00:03"']),
]


def symbol(firmware, name):
    """The address and size of a symbol in the firmware."""
    listing = subprocess.run(["nm", "-S", "-C", str(firmware)], capture_output=True, text=True, check=True)
    for line in listing.stdout.splitlines():
        parts = line.split(None, 3)
        if len(parts) == 4 and parts[3] == name:
            return int(parts[0], 16), int(parts[1], 16)
    sys.exit(f"{firmware} has no {name}")


def emulator_folder():
    if "PINETIME_EMULATOR" in os.environ:
        return Path(os.environ["PINETIME_EMULATOR"])
    common = subprocess.run(
        ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
        capture_output=True, text=True, cwd=REPOSITORY,
    ).stdout.strip()
    return Path(common).parent.parent / "PineTimeEmulator"


def library_lines(library, folder):
    """Emulator lines that load the image header and the library behind the ELF, where the firmware finds it."""
    header = Path(folder) / "header.bin"
    address = subprocess.run(
        [sys.executable, "-I", str(REPOSITORY / "tools" / "image_header.py"), "--library-beside-elf", str(library), str(header)],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    return [
        f"sysbus LoadBinary @{header} {FIRMWARE_VECTOR_TABLE - IMAGE_HEADER_SIZE:#x}",
        f"sysbus LoadBinary @{Path(library).resolve()} {address}",
    ]


# ============================================================================
# Running it
# ============================================================================

def main():
    firmware, flash, library = (Path(argument).resolve() for argument in sys.argv[1:4])
    screenshots = Path(sys.argv[4]).resolve() if len(sys.argv) > 4 else None
    start_seconds = int(sys.argv[5]) if len(sys.argv) > 5 else START_SECONDS
    heap, heap_size = symbol(firmware, HEAP_SYMBOL)
    memory, memory_size = symbol(firmware, HEAP_MEMORY_SYMBOL)
    emulator = emulator_folder()

    with tempfile.TemporaryDirectory() as folder:
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
        lines.append(f'emulation RunFor "00:00:{start_seconds:02}"')
        for name, commands in STEPS:
            lines += commands
            lines += [f"sysbus ReadBytes {heap:#x} {heap_size:#x}", f"sysbus ReadBytes {memory:#x} {memory_size:#x}"]
            if screenshots:
                lines.append(f"lcd SaveScreenshot @{screenshots / (name + '.png')}")
        lines.append("quit")
        script = Path(folder) / "heap.resc"
        script.write_text("\n".join(lines) + "\n")
        run = subprocess.run(
            ["nix", "develop", str(emulator), "-c", "pinetime-engine", str(script)],
            capture_output=True, text=True, cwd=emulator,
        )

    read = bytes(int(byte, 16) for byte in re.findall(r"0x([0-9A-Fa-f]{2})\b", run.stdout))
    step_bytes = heap_size + memory_size
    if len(read) < step_bytes * len(STEPS):
        sys.exit("the emulator gave too few readings:\n" + run.stdout[-2000:] + run.stderr[-2000:])
    print(f"heap: {memory_size} bytes at {memory:#x}")
    for index, (name, _) in enumerate(STEPS):
        step = read[index * step_bytes:(index + 1) * step_bytes]
        used = struct.unpack_from("<I", step, USED_WORD * 4)[0]
        heap_memory = step[heap_size:]
        touched = max((offset + 4 for offset in range(0, memory_size, 4) if any(heap_memory[offset:offset + 4])), default=0)
        print(f"{name:9} {used:6} bytes in use, highest byte touched {touched}")


main()
