"""Measures a firmware's peak stack use in the PineTime emulator; see docs/memory-map.md.

It presets the UICR words embassy-nrf writes on the first start, so the emulated chip is not reset
(a reset clears RAM). It then fills the stack, from `_stack_end` to the top of RAM, with a pattern,
starts the firmware, can swipe up to the launcher, and reports the lowest stack word that changed.

    python3 -I tools/measure_stack.py firmware/target/thumbv7em-none-eabihf/release/neotime-firmware [seconds] [--launcher]

Run it inside `nix develop`. It finds the emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator
folder beside this repository. Cap its memory with `ulimit -v 8000000` first: Renode can grow.
"""

import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parent.parent
STACK_TOP = 0x2000FFF0
PATTERN = 0xA5A5A5A5
FIRMWARE_VECTOR_TABLE = 0x5000
SECONDS_BEFORE_SWIPE = 4

# The UICR words embassy-nrf writes on the first start: both reset pin selections and the NFC pins as GPIO.
FIRST_START_UICR = [(0x10001200, 0x15), (0x10001204, 0x15), (0x1000120C, 0xFFFFFFFE)]

# The stack is painted one word at a time, and read back every 16 bytes.
READ_STEP = 16
# The last words below the top hold the alive marker, so they are left alone.
TOP_LEFT_ALONE = 0x80


def emulator_folder():
    if "PINETIME_EMULATOR" in os.environ:
        candidates = [Path(os.environ["PINETIME_EMULATOR"])]
    else:
        # From a git worktree the repository's own folder is the main checkout, not this one.
        common = subprocess.run(
            ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
            capture_output=True, text=True, cwd=REPOSITORY,
        ).stdout.strip()
        main_checkout = Path(common).parent if common else REPOSITORY
        candidates = [REPOSITORY.parent / "PineTimeEmulator", main_checkout.parent / "PineTimeEmulator"]
    for folder in candidates:
        if (folder / "scripts" / "pinetime.resc").is_file():
            return folder
    sys.exit(f"no PineTime emulator at {candidates[-1]}; set PINETIME_EMULATOR")


def stack_end(firmware):
    symbols = subprocess.run(["nm", str(firmware)], capture_output=True, text=True, check=True).stdout
    for line in symbols.splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[2] == "_stack_end":
            return int(parts[0], 16)
    sys.exit(f"{firmware} has no _stack_end symbol")


def renode_script(emulator, firmware, end, seconds, swipe_to_launcher):
    painted = range(end, STACK_TOP - TOP_LEFT_ALONE, 4)
    lines = [f"include @{emulator}/scripts/pinetime.resc"]
    lines += [f"sysbus WriteDoubleWord {address:#x} {value:#x}" for address, value in FIRST_START_UICR]
    lines.append(f"sysbus LoadELF @{firmware}")
    lines += [f"sysbus WriteDoubleWord {address:#x} {PATTERN:#x}" for address in painted]
    lines += [
        f"cpu VectorTableOffset {FIRMWARE_VECTOR_TABLE:#x}",
        f"cpu SP `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE:#x}`",
        f"cpu PC `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE + 4:#x}`",
    ]
    if swipe_to_launcher:
        lines += [f'emulation RunFor "00:00:{SECONDS_BEFORE_SWIPE:02}"', 'sysbus.serial1.touch Swipe "up"']
    lines += [f'emulation RunFor "00:00:{seconds:05.2f}"', "cpu PC"]
    lines += [f"sysbus ReadDoubleWord {address:#x}" for address in range(end, STACK_TOP - TOP_LEFT_ALONE, READ_STEP)]
    lines.append("quit")
    return "\n".join(lines) + "\n"


def main():
    arguments = [argument for argument in sys.argv[1:] if not argument.startswith("--")]
    if not arguments:
        sys.exit(__doc__)
    firmware = Path(arguments[0]).resolve()
    seconds = float(arguments[1]) if len(arguments) > 1 else 8.0
    swipe_to_launcher = "--launcher" in sys.argv
    emulator = emulator_folder()
    end = stack_end(firmware)

    with tempfile.NamedTemporaryFile("w", suffix=".resc", delete=False) as script:
        script.write(renode_script(emulator, firmware, end, seconds, swipe_to_launcher))
    run = subprocess.run(
        ["nix", "develop", str(emulator), "-c", "renode", "--console", "--disable-xwt", "--plain", script.name],
        capture_output=True,
        text=True,
        cwd=emulator,
    )
    os.unlink(script.name)

    output = run.stdout.splitlines()
    resets = [line for line in output if "Resetting the chip" in line or "Watchdog timed out" in line]
    words = [int(line, 16) for line in output if line.startswith("0x")]
    if not words:
        sys.exit("the emulator gave no readings:\n" + run.stdout[-2000:] + run.stderr[-2000:])
    print(f"stack: {end:#x} to {STACK_TOP:#x}, {STACK_TOP - end} bytes")
    print(f"final PC {words[0]:#x}; resets and watchdog timeouts: {len(resets)}")
    changed = next((index for index, word in enumerate(words[1:]) if word != PATTERN), None)
    if changed is None:
        print("no stack word changed")
        return
    lowest = end + changed * READ_STEP
    print(f"peak stack use about {STACK_TOP - lowest} bytes, {lowest - end} bytes spare")
    if lowest == end:
        print("the whole stack was used: it probably overflowed")


main()
