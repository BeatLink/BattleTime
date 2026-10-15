"""Checks in the PineTime emulator that the watch shows the face the watch_face setting chooses after a restart.

It installs two faces, analog and digital, on the emulated external flash and starts the firmware
three times: with no face chosen the first face in id order, analog, must open; with digital chosen
in the saved settings, digital must open; and with a face chosen that is not installed, the watch
must fall back to analog.

    python3 -I tools/check_watch_face.py FIRMWARE_ELF

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

FACE_IDS = ["analog", "digital"]
FIRMWARE_VECTOR_TABLE = 0x8100
IMAGE_HEADER_SIZE = 256
FIRST_START_UICR = [(0x10001200, 0x15), (0x10001204, 0x15), (0x1000120C, 0xFFFFFFFE)]
OPEN_APP_SYMBOL = "battletime_firmware::restart::OPEN_APP"

# The settings file: its format byte, then records of a setting's number and its value.
SETTINGS_FILE_FORMAT = 1
WATCH_FACE_SETTING = 18

SECONDS_TO_OPEN = 120


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


def face_number(face_id):
    """The number the watch_face setting holds for a face: the id's 32-bit FNV-1a hash with the top bit cleared, never 0."""
    number = 0x811C9DC5
    for byte in face_id.encode():
        number = ((number ^ byte) * 0x01000193) & 0xFFFFFFFF
    return (number & 0x7FFFFFFF) or 1


def build_library(folder):
    """Compiles the two faces and the shared library; gives the faces' images and the library."""
    run(["cargo", "build", "-q", "-p", "battletime-compiler"])
    compiler = str(BATTLETIME / "target" / "debug" / "battletime-compiler")
    faces = {}
    for face_id in FACE_IDS:
        faces[face_id] = folder / f"{face_id}.ntapp"
        run([compiler, f"faces/{face_id}/face.tsx", "-o", str(faces[face_id])])
    library = folder / "library.bin"
    run([compiler, "library", "-o", str(library)])
    return faces, library


def build_flash(folder, faces, chosen_face_id):
    """Writes a flash image with both faces and a settings file choosing `chosen_face_id`, or none."""
    files = [f"{face_id}={image}" for face_id, image in faces.items()]
    if chosen_face_id is not None:
        settings = folder / f"settings-{chosen_face_id}.bin"
        record = struct.pack("<Bi", WATCH_FACE_SETTING, face_number(chosen_face_id))
        settings.write_bytes(bytes([SETTINGS_FILE_FORMAT]) + record)
        files.append(f"/settings/system={settings}")
    flash = folder / f"flash-{chosen_face_id}.bin"
    run(["cargo", "run", "-q", "-p", "battletime-packages", "--example", "filesystem_use", "--",
         "--no-theme", "--image", str(flash), *files])
    return flash


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

    def read_bytes(self, address, size):
        reply = self.command(f"sysbus ReadBytes {address:#x} {size:#x}")
        return bytes(int(byte, 16) for byte in re.findall(r"0x([0-9A-Fa-f]{2})\b", reply))

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


def face_opened(emulator, firmware, folder, library, flash):
    """Starts the firmware on this flash and gives the id of the first face it opens, before which the record holds leftovers."""
    open_app_address, open_app_size = symbol(firmware, OPEN_APP_SYMBOL)
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
    script = folder / "face.resc"
    script.write_text("\n".join(lines) + "\n")
    engine = Engine(emulator, script)
    try:
        end = time.monotonic() + SECONDS_TO_OPEN
        while time.monotonic() < end:
            opened = engine.read_bytes(open_app_address, open_app_size).split(b"\0")[0].decode(errors="replace")
            if opened in FACE_IDS:
                return opened
            time.sleep(0.2)
        return ""
    finally:
        engine.stop()


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    firmware = Path(sys.argv[1]).resolve()
    emulator = emulator_folder()
    results = []

    with tempfile.TemporaryDirectory(prefix="check-face-") as temporary:
        folder = Path(temporary)
        faces, library = build_library(folder)
        for chosen, expected in [(None, "analog"), ("digital", "digital"), ("casio", "analog")]:
            flash = build_flash(folder, faces, chosen)
            opened = face_opened(emulator, firmware, folder, library, flash)
            results.append(check(f"with {chosen or 'no face'} chosen", opened == expected,
                                 f"opened {opened or 'nothing'}, expected {expected}"))

    sys.exit(0 if all(results) else 1)


main()
