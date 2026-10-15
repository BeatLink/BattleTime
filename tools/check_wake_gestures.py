"""Checks the wake gestures end to end in the PineTime emulator: raise to wake, lower to sleep and the face's double tap.

It installs a face on the emulated external flash with settings that turn raise to wake and lower
to sleep on and the screen timeout down to 5 seconds. Once the screen times out it tilts the
emulated accelerometer as a wrist being raised and checks that the screen lights, then as an arm
being lowered and checks that it goes dark well before the timeout. Last it lights the screen with
the button, taps the face twice and checks that the screen goes dark.

    python3 -I tools/check_wake_gestures.py FIRMWARE_ELF

Run it inside `nix develop`, with the firmware built (`cd battletime/firmware && cargo build
--release`). It finds the emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator folder beside
the main checkout, and takes about a minute.
"""

import os
import re
import socket
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parent.parent
BATTLETIME = REPOSITORY / "battletime"

# ============================================================================
# The install
# ============================================================================

FACE_ID = "digital"
FIRMWARE_VECTOR_TABLE = 0x8100
IMAGE_HEADER_SIZE = 256
FIRST_START_UICR = [(0x10001200, 0x15), (0x10001204, 0x15), (0x1000120C, 0xFFFFFFFE)]
OPEN_APP_SYMBOL = "battletime_firmware::restart::OPEN_APP"
COUNTERS_SYMBOL = "battletime_firmware::diagnostics::COUNTERS"
SCREEN_LIT_COUNTER = 2

# The settings file: its format byte, then records of a setting's number and its value.
SETTINGS_FILE_FORMAT = 1
SCREEN_TIMEOUT_SECONDS = (4, 5)
RAISE_TO_WAKE = (13, 1)
LOWER_TO_SLEEP = (14, 1)

# Readings in milli-g along the chip's own axes, whose x is the watch's reversed y.
LYING_FLAT = (0, 0, -1000)
RAISED = (-781, 0, -625)
TILTED = (293, 0, -957)
HANGING = (977, 0, -195)

SECONDS_TO_TIME_OUT = 12
SECONDS_TO_REACT = 1.5
SECONDS_HELD = 1.0


def run(command, folder=BATTLETIME):
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
    """Compiles the face and the shared library, and writes the flash image with the settings file; gives the flash and the library."""
    run(["cargo", "build", "-q", "-p", "battletime-compiler"])
    compiler = str(BATTLETIME / "target" / "debug" / "battletime-compiler")
    face = folder / "face.ntapp"
    run([compiler, "faces/digital/face.tsx", "-o", str(face)])
    library = folder / "library.bin"
    run([compiler, "library", "-o", str(library)])
    settings = folder / "settings.bin"
    records = b"".join(struct.pack("<Bi", number, value)
                       for number, value in [SCREEN_TIMEOUT_SECONDS, RAISE_TO_WAKE, LOWER_TO_SLEEP])
    settings.write_bytes(bytes([SETTINGS_FILE_FORMAT]) + records)
    flash = folder / "flash.bin"
    run(["cargo", "run", "-q", "-p", "battletime-packages", "--example", "filesystem_use", "--",
         "--no-theme", "--image", str(flash), f"{FACE_ID}={face}", f"/settings/system={settings}"])
    return flash, library


def library_lines(library, folder):
    """Emulator lines that load the image header and the library behind the ELF, where the firmware finds it."""
    header = folder / "header.bin"
    address = run([sys.executable, "-I", str(REPOSITORY / "tools" / "image_header.py"),
                   "--library-beside-elf", str(library), str(header)]).strip()
    return [
        f"sysbus LoadBinary @{header} {FIRMWARE_VECTOR_TABLE - IMAGE_HEADER_SIZE:#x}",
        f"sysbus LoadBinary @{library} {address}",
    ]


# ============================================================================
# The engine
# ============================================================================

class Engine:
    """A `pinetime-engine` with its monitor on a TCP port."""

    PROMPT = b"(pinetime) "

    def __init__(self, emulator, script):
        port = free_port()
        self.process = subprocess.Popen(
            ["nix", "develop", str(emulator), "-c", "pinetime-engine", "--port", str(port), str(script)],
            cwd=emulator, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
        )
        self.connection = self.connect(port)
        self.pending = b""
        self.read_reply()

    def connect(self, port):
        deadline = time.monotonic() + 600
        while True:
            try:
                return socket.create_connection(("127.0.0.1", port), timeout=120)
            except OSError:
                if self.process.poll() is not None or time.monotonic() > deadline:
                    sys.exit("pinetime-engine did not open its monitor port:\n" + self.process.stderr.read().decode())
                time.sleep(0.2)

    def read_reply(self):
        while self.PROMPT not in self.pending:
            chunk = self.connection.recv(1 << 20)
            if not chunk:
                raise RuntimeError("pinetime-engine closed its monitor")
            self.pending += chunk
        reply, self.pending = self.pending.split(self.PROMPT, 1)
        return reply.decode(errors="replace").strip()

    def command(self, line):
        self.connection.sendall(line.encode() + b"\n")
        reply = self.read_reply()
        if reply.startswith("error:"):
            raise RuntimeError(f"{line}: {reply}")
        return reply

    def emulated_seconds(self):
        return float(self.command("emulation ElapsedTime").split()[0])

    def wait_emulated(self, seconds):
        until = self.emulated_seconds() + seconds
        while self.emulated_seconds() < until:
            time.sleep(0.05)

    def read_bytes(self, address, size):
        reply = self.command(f"sysbus ReadBytes {address:#x} {size:#x}")
        return bytes(int(byte, 16) for byte in re.findall(r"0x([0-9A-Fa-f]{2})\b", reply))

    def tilt(self, reading):
        self.command("accelerometer SetAcceleration {} {} {}".format(*reading))

    def stop(self):
        try:
            self.command("quit")
        except (OSError, RuntimeError):
            pass
        try:
            self.process.wait(30)
        except subprocess.TimeoutExpired:
            self.process.kill()


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


# ============================================================================
# Checks
# ============================================================================

def check(name, passed, detail):
    print(f"{'ok  ' if passed else 'FAIL'}  {name}: {detail}")
    return passed


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    firmware = Path(sys.argv[1]).resolve()
    open_app_address, open_app_size = symbol(firmware, OPEN_APP_SYMBOL)
    counters_address, _ = symbol(firmware, COUNTERS_SYMBOL)
    screen_lit_address = counters_address + 4 * SCREEN_LIT_COUNTER
    emulator = emulator_folder()
    results = []

    with tempfile.TemporaryDirectory(prefix="check-wake-") as temporary:
        folder = Path(temporary)
        flash, library = build_install(folder)
        lines = [f"include @{emulator}/scripts/pinetime.resc"]
        lines += [f"sysbus WriteDoubleWord {address:#x} {value:#x}" for address, value in FIRST_START_UICR]
        lines += [
            f"sysbus LoadELF @{firmware}",
            *library_lines(library, folder),
            f"externalFlashMemory LoadBinary @{flash} 0",
            f"cpu VectorTableOffset {FIRMWARE_VECTOR_TABLE:#x}",
            f"cpu SP `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE:#x}`",
            f"cpu PC `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE + 4:#x}`",
            "start",
        ]
        script = folder / "wake.resc"
        script.write_text("\n".join(lines) + "\n")

        engine = Engine(emulator, script)

        def is_lit():
            return struct.unpack("<I", engine.read_bytes(screen_lit_address, 4))[0] != 0

        def wait_for_screen(lit, seconds):
            end = engine.emulated_seconds() + seconds
            while is_lit() != lit and engine.emulated_seconds() < end:
                time.sleep(0.05)
            return is_lit() == lit

        try:
            end = time.monotonic() + 120
            while engine.read_bytes(open_app_address, open_app_size).split(b"\0")[0] != FACE_ID.encode():
                if time.monotonic() > end:
                    sys.exit("the face never opened")
                time.sleep(0.2)
            engine.tilt(LYING_FLAT)
            results.append(check("screen times out", wait_for_screen(False, SECONDS_TO_TIME_OUT), "after 5 s"))

            engine.wait_emulated(SECONDS_HELD)
            engine.tilt(RAISED)
            results.append(check("raising the wrist wakes it", wait_for_screen(True, SECONDS_TO_REACT),
                                 f"within {SECONDS_TO_REACT} s"))

            engine.tilt(TILTED)
            engine.wait_emulated(SECONDS_HELD)
            engine.tilt(HANGING)
            results.append(check("lowering the arm puts it to sleep", wait_for_screen(False, SECONDS_TO_REACT),
                                 f"within {SECONDS_TO_REACT} s, before the timeout"))

            engine.tilt(LYING_FLAT)
            engine.command("sideButton Hold 0.1")
            woke = wait_for_screen(True, SECONDS_TO_REACT)
            engine.wait_emulated(SECONDS_HELD)
            engine.command("touch Tap 120 120")
            engine.wait_emulated(0.2)
            engine.command("touch Tap 120 120")
            results.append(check("a double tap on the face puts it to sleep",
                                 woke and wait_for_screen(False, SECONDS_TO_REACT), f"within {SECONDS_TO_REACT} s"))
        finally:
            engine.stop()

    sys.exit(0 if all(results) else 1)


main()
