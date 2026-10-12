"""Measures a firmware's peak stack use in the PineTime emulator; see docs/memory-map.md.

It presets the UICR words embassy-nrf writes on the first start, so the emulated chip is not reset
(a reset clears RAM). It then fills the stack, from `_stack_end` to the top of RAM, with a pattern,
starts the firmware, can swipe up to the launcher, and reports the lowest stack word that changed.

    python3 -I tools/measure_stack.py FIRMWARE_ELF [seconds] [--launcher] [--flash IMAGE] [--library FILE]
                                      [--packages LINE ...]

`--flash` loads a whole external flash image, such as `filesystem_use --image` writes, so apps are
installed; `--library` puts a compiled shared library where the firmware looks for it.
`--packages` runs the emulator with its Bluetooth bridge, connects to the watch through
`tools/packages_over_hci.py` and sends each line to the packages characteristic, as the companion
app would, before reading the stack. Spare bytes are counted down to the top of the stack guard,
the lowest `GUARD_BYTES` the memory protection unit forbids.

Run it inside `nix develop`. It finds the emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator
folder beside this repository, and runs it with that checkout's `pinetime-engine`. The engine has no
Bluetooth bridge yet, so `--packages` runs the Renode emulator it replaced instead: a PineTimeEmulator
checkout at 97ebb02, named by $PINETIME_RENODE_EMULATOR. Cap its memory with `ulimit -v 12000000`
first, since Renode can grow.
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
FIRMWARE_VECTOR_TABLE = 0x5000
SECONDS_BEFORE_SWIPE = 4
# Where the shared library goes, behind `NTLB` and its length; see services/packages/src/library.rs.
SHARED_LIBRARY_START = 0x69000
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


def renode_emulator_folder():
    """The pinned Renode emulator, which alone has the Bluetooth bridge `--packages` needs."""
    folder = Path(os.environ.get("PINETIME_RENODE_EMULATOR", ""))
    if not (folder / "scripts" / "bluetooth-bridge.resc").is_file():
        sys.exit("--packages needs the Renode emulator: set PINETIME_RENODE_EMULATOR to a PineTimeEmulator checkout at 97ebb02")
    return folder


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

def library_file(library, folder):
    """The library behind its `NTLB` header, as the firmware finds it in internal flash."""
    image = Path(library).read_bytes()
    path = Path(folder) / "library.bin"
    path.write_bytes(b"NTLB" + struct.pack("<I", len(image)) + image)
    return path


def start_lines(emulator, firmware, end, flash, library):
    painted = range(end, STACK_TOP - TOP_LEFT_ALONE, 4)
    lines = [f"include @{emulator}/scripts/pinetime.resc"]
    lines += [f"sysbus WriteDoubleWord {address:#x} {value:#x}" for address, value in FIRST_START_UICR]
    lines.append(f"sysbus LoadELF @{firmware}")
    if library:
        lines.append(f"sysbus LoadBinary @{library} {SHARED_LIBRARY_START:#x}")
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

def run_in_one_go(emulator, lines, end, seconds, swipe_to_launcher):
    """Runs the emulator for a fixed time without Bluetooth, and gives back its readings."""
    if swipe_to_launcher:
        lines += [f'emulation RunFor "00:00:{SECONDS_BEFORE_SWIPE:02}"', 'sysbus.serial1.touch Swipe "up"']
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
    """Renode's monitor on a TCP port, one command at a time."""

    def __init__(self, port):
        self.socket = socket.create_connection(("127.0.0.1", port))
        self.socket.settimeout(0.3)
        self.text = ""

    def run(self, line, seconds=60):
        """Sends a command and gives back what it printed, up to the prompt that follows it."""
        self.socket.sendall(line.encode() + b"\n")
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            try:
                self.text += self.socket.recv(65536).decode(errors="replace")
            except socket.timeout:
                pass
            clean = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", self.text)
            echoed = clean.rfind(line)
            if echoed >= 0:
                answer = clean[echoed + len(line):]
                prompt = re.search(r"\(\w+\)\s*$", answer)
                if prompt:
                    self.text = ""
                    return answer[: prompt.start()]
        raise TimeoutError(f"the monitor never answered {line}")


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def run_with_bluetooth(emulator, lines, end, seconds, swipe_to_launcher, packages):
    """Runs the emulator in real time with its Bluetooth bridge, sends the packages lines, and gives back its readings."""
    folder = Path(tempfile.mkdtemp(prefix="measure-stack-"))
    hci = folder / "hci"
    lines += ["mach clear", f'$hci_pty="{hci}"', f"include @{emulator}/scripts/bluetooth-bridge.resc", "start"]
    script = folder / "start.resc"
    script.write_text("\n".join(lines) + "\n")
    port = free_port()
    renode = subprocess.Popen(
        ["nix", "develop", str(emulator), "-c", "renode", "--disable-xwt", "--plain", "-P", str(port), str(script)],
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
        monitor = Monitor(port)
        if swipe_to_launcher:
            time.sleep(SECONDS_BEFORE_SWIPE)
            monitor.run('sysbus.serial1.touch Swipe "up"')
        for line, reply in zip(packages, packages_over_hci.run_commands(str(hci), packages)):
            print(f"{line} -> {reply}")
        time.sleep(seconds)
        words = []
        for line in reading_lines(end)[:3]:
            found = re.findall(r"0x[0-9A-Fa-f]+", monitor.run(line))
            words.append(int(found[0], 16) if found else 0)
        # The stack in one read, sampled every READ_STEP bytes as the other way reads it.
        length = STACK_TOP - TOP_LEFT_ALONE - end
        command = f"sysbus ReadBytes {end:#x} {length:#x}"
        text = monitor.run(command)
        stack = bytes(int(byte, 16) for byte in re.findall(r"0x([0-9A-Fa-f]{2})\b", text))
        words += [int.from_bytes(stack[offset:offset + 4], "little") for offset in range(0, len(stack) - 3, READ_STEP)]
        monitor.socket.sendall(b"quit\n")
        return words
    finally:
        renode.terminate()
        try:
            renode.wait(30)
        except subprocess.TimeoutExpired:
            renode.kill()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("firmware")
    parser.add_argument("seconds", nargs="?", type=float, default=8.0)
    parser.add_argument("--launcher", action="store_true", help="swipe up to the launcher")
    parser.add_argument("--flash", help="an external flash image to load")
    parser.add_argument("--library", help="a compiled shared library to load")
    parser.add_argument("--packages", nargs="+", metavar="LINE", help="packages command lines to send over Bluetooth")
    arguments = parser.parse_args()

    firmware = Path(arguments.firmware).resolve()
    emulator = renode_emulator_folder() if arguments.packages else emulator_folder()
    end = stack_end(firmware)
    with tempfile.TemporaryDirectory() as folder:
        library = library_file(arguments.library, folder) if arguments.library else None
        lines = start_lines(emulator, firmware, end, arguments.flash, library)
        if arguments.packages:
            words = run_with_bluetooth(emulator, lines, end, arguments.seconds, arguments.launcher, arguments.packages)
        else:
            words = run_in_one_go(emulator, lines, end, arguments.seconds, arguments.launcher)

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
