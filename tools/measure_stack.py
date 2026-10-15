"""Measures a firmware's peak stack use in the PineTime emulator; see docs/memory-map.md.

It presets the UICR words embassy-nrf writes on the first start, so the emulated chip is not reset
(a reset clears RAM). It then fills the stack, from `_stack_end` to the top of RAM, with a pattern,
starts the firmware, can swipe up to the launcher and open its first app, and reports the lowest stack word that changed.

    python3 -I tools/measure_stack.py FIRMWARE_ELF [seconds] [--launcher [--open-first-app]] [--flash IMAGE] [--library FILE]
                                      [--packages LINE ...] [--then TOUCH ...]

`--then` sends more touches after those, each two seconds after the last, written as the
emulator's touch commands take them, such as `Tap 120 120` or `Swipe "left"`.

`--flash` loads a whole external flash image, such as `filesystem_use --image` writes, so apps are
installed; `--library` puts a compiled shared library where the firmware looks for it.
`--packages` runs the emulator with its Bluetooth bridge, connects to the watch through
`tools/packages_over_hci.py` and sends each line to the packages characteristic, as the companion
app would, before reading the stack. Spare bytes are counted down to the top of the stack guard,
the lowest `GUARD_BYTES` the memory protection unit forbids.

Run it inside `nix develop`. It finds the emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator
folder beside this repository, and runs it with that checkout's `pinetime-engine`; `--packages` runs
the engine with `--bluetooth`, its HCI controller on a pseudo-terminal.
"""

import argparse
import os
import re
import socket
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

# The client sits beside this script; importing it should leave no bytecode in the repository.
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import packages_over_hci  # noqa: E402

REPOSITORY = Path(__file__).resolve().parent.parent
STACK_TOP = 0x2000FFF0
BOOT_COUNT = 0x2000FFF4
# RESETREAS: a non-zero value means the chip reset during the run, which clears the stack pattern.
RESET_REASON = 0x40000400
PATTERN = 0xA5A5A5A5
FIRMWARE_VECTOR_TABLE = 0x8100
SECONDS_BEFORE_SWIPE = 4
IMAGE_HEADER_SIZE = 256
SECONDS_BEFORE_TAP = 3
SECONDS_BETWEEN_TOUCHES = 2
# The middle of the launcher's first row.
FIRST_APP_TAP = (120, 40)
# As in firmware/src/stack_guard.rs.
GUARD_BYTES = 256

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


def guard_top(end):
    """The first address above the stack guard, which starts at the first `GUARD_BYTES` boundary at or above `end`."""
    return -(-end // GUARD_BYTES) * GUARD_BYTES + GUARD_BYTES


# ============================================================================
# The emulator script
# ============================================================================

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


def start_lines(emulator, firmware, end, flash, library, folder):
    painted = range(end, STACK_TOP - TOP_LEFT_ALONE, 4)
    lines = [f"include @{emulator}/scripts/pinetime.resc"]
    lines += [f"sysbus WriteDoubleWord {address:#x} {value:#x}" for address, value in FIRST_START_UICR]
    lines.append(f"sysbus LoadELF @{firmware}")
    if library:
        lines += library_lines(library, folder)
    if flash:
        lines.append(f"externalFlashMemory LoadBinary @{Path(flash).resolve()} 0")
    lines += [f"sysbus WriteDoubleWord {address:#x} {PATTERN:#x}" for address in painted]
    lines += [
        f"cpu VectorTableOffset {FIRMWARE_VECTOR_TABLE:#x}",
        f"cpu SP `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE:#x}`",
        f"cpu PC `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE + 4:#x}`",
    ]
    return lines


def reading_lines(end):
    return ["cpu PC", f"sysbus ReadDoubleWord {RESET_REASON:#x}", f"sysbus ReadDoubleWord {BOOT_COUNT:#x}"] + [
        f"sysbus ReadDoubleWord {address:#x}" for address in range(end, STACK_TOP - TOP_LEFT_ALONE, READ_STEP)
    ]


# ============================================================================
# Running it
# ============================================================================

def run_in_one_go(emulator, lines, end, seconds, swipe_to_launcher, open_first_app, touches=()):
    """Runs the emulator for a fixed time without Bluetooth, and gives back its readings."""
    if swipe_to_launcher:
        lines += [f'emulation RunFor "00:00:{SECONDS_BEFORE_SWIPE:02}"', 'sysbus.serial1.touch Swipe "up"']
    if open_first_app:
        x, y = FIRST_APP_TAP
        lines += [f'emulation RunFor "00:00:{SECONDS_BEFORE_TAP:02}"', f"sysbus.serial1.touch Tap {x} {y}"]
    for touch in touches:
        lines += [f'emulation RunFor "00:00:{SECONDS_BETWEEN_TOUCHES:02}"', f"sysbus.serial1.touch {touch}"]
    lines += [f'emulation RunFor "00:00:{seconds:05.2f}"'] + reading_lines(end) + ["quit"]
    with tempfile.NamedTemporaryFile("w", suffix=".resc", delete=False) as script:
        script.write("\n".join(lines) + "\n")
    run = subprocess.run(
        ["nix", "develop", str(emulator), "-c", "pinetime-engine", script.name],
        capture_output=True,
        text=True,
        cwd=emulator,
    )
    os.unlink(script.name)
    words = [int(line, 16) for line in run.stdout.splitlines() if line.startswith("0x")]
    if not words:
        sys.exit("the emulator gave no readings:\n" + run.stdout[-2000:] + run.stderr[-2000:])
    return words


class Monitor:
    """The engine's monitor on a TCP port, one command at a time, each answered before its prompt."""

    PROMPT = "(pinetime) "

    def __init__(self, port):
        self.socket = socket.create_connection(("127.0.0.1", port))
        self.socket.settimeout(60)
        self.file = self.socket.makefile("r")
        self.read_to_prompt()

    def read_to_prompt(self):
        text = ""
        while not text.endswith(self.PROMPT):
            character = self.file.read(1)
            if not character:
                raise EOFError("the emulator's monitor closed")
            text += character
        return text[: -len(self.PROMPT)].strip()

    def run(self, line):
        """Sends a command and gives back what it printed."""
        self.socket.sendall(line.encode() + b"\n")
        return self.read_to_prompt()


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def run_with_bluetooth(emulator, lines, end, seconds, swipe_to_launcher, packages):
    """Runs the emulator in real time with its Bluetooth bridge, sends the packages lines, and gives back its readings."""
    folder = Path(tempfile.mkdtemp(prefix="measure-stack-"))
    hci = folder / "hci"
    script = folder / "start.resc"
    script.write_text("\n".join(lines + ["start"]) + "\n")
    port = free_port()
    # The engine reads its standard input, so it gets one that blocks rather than ends.
    waiting_input = subprocess.Popen(["sleep", "infinity"], stdout=subprocess.PIPE)
    engine = subprocess.Popen(
        ["nix", "develop", str(emulator), "-c", "pinetime-engine", "--port", str(port), "--bluetooth", str(hci), str(script)],
        stdin=waiting_input.stdout,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        cwd=emulator,
    )
    try:
        for _ in range(600):
            if hci.exists():
                break
            time.sleep(0.5)
        else:
            sys.exit("the emulator never opened its Bluetooth bridge")
        time.sleep(1)
        monitor = Monitor(port)
        if swipe_to_launcher:
            time.sleep(SECONDS_BEFORE_SWIPE)
            monitor.run('sysbus.serial1.touch Swipe "up"')
        for line, reply in zip(packages, packages_over_hci.run_commands(str(hci), packages)):
            print(f"{line} -> {reply}")
        time.sleep(seconds)
        words = []
        for line in reading_lines(end):
            found = re.findall(r"0x[0-9A-Fa-f]+", monitor.run(line))
            words.append(int(found[0], 16) if found else 0)
        monitor.socket.sendall(b"quit\n")
        return words
    finally:
        engine.terminate()
        try:
            engine.wait(30)
        except subprocess.TimeoutExpired:
            engine.kill()
        waiting_input.kill()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("firmware")
    parser.add_argument("seconds", nargs="?", type=float, default=8.0)
    parser.add_argument("--launcher", action="store_true", help="swipe up to the launcher")
    parser.add_argument("--open-first-app", action="store_true", help="then tap the launcher's first row")
    parser.add_argument("--flash", help="an external flash image to load")
    parser.add_argument("--library", help="a compiled shared library to load")
    parser.add_argument("--packages", nargs="+", metavar="LINE", help="packages command lines to send over Bluetooth")
    parser.add_argument("--then", nargs="+", default=[], metavar="TOUCH", help="touch commands to send next, such as 'Tap 120 120'")
    arguments = parser.parse_args()

    firmware = Path(arguments.firmware).resolve()
    emulator = emulator_folder()
    end = stack_end(firmware)
    with tempfile.TemporaryDirectory() as folder:
        lines = start_lines(emulator, firmware, end, arguments.flash, arguments.library, folder)
        if arguments.packages:
            words = run_with_bluetooth(emulator, lines, end, arguments.seconds, arguments.launcher, arguments.packages)
        else:
            words = run_in_one_go(
                emulator, lines, end, arguments.seconds, arguments.launcher, arguments.open_first_app, arguments.then
            )

    usable_end = guard_top(end)
    print(f"stack: {end:#x} to {STACK_TOP:#x}, {STACK_TOP - usable_end} bytes above the {GUARD_BYTES}-byte guard")
    print(f"final PC {words[0]:#x}; reset reasons {words[1]:#x}; starts {words[2]}")
    if words[1]:
        print("the chip reset during the run, so the stack pattern was cleared")
    stack_words = words[3:]
    changed = next((index for index, word in enumerate(stack_words) if word != PATTERN), None)
    if changed is None:
        print("no stack word changed")
        return
    lowest = end + changed * READ_STEP
    print(f"peak stack use about {STACK_TOP - lowest} bytes, {lowest - usable_end} bytes spare")
    if lowest < usable_end:
        print("the stack reached the guard: it overflowed")


main()
