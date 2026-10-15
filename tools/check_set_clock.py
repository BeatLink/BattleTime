"""Checks setting the clock by hand end to end in the PineTime emulator, through the Settings app's Date & time pages.

It installs the digital face, the launcher and the Settings app on the emulated external flash.
Once the face shows it reads the clock the firmware keeps across a reset, opens Settings from the
launcher, scrolls to Date & time, moves the year on by one and the hour on by one, and taps Set.
It then checks that the kept clock reads that local time at 00 seconds, counting on from there,
and leaves a picture of the face afterwards in the folder given, if one is.

    python3 -I tools/check_set_clock.py FIRMWARE_ELF [PICTURE_FOLDER]

Run it inside `nix develop`, with the firmware built (`cd battletime/firmware && cargo build
--release`). It finds the emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator folder beside
the main checkout, and takes about two minutes.
"""

import calendar
import os
import re
import socket
import struct
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parent.parent
BATTLETIME = REPOSITORY / "battletime"

# ============================================================================
# The install
# ============================================================================

FACE_ID = "digital"
LAUNCHER_ID = "launcher"
SETTINGS_ID = "settings"
FIRMWARE_VECTOR_TABLE = 0x8100
IMAGE_HEADER_SIZE = 256
FIRST_START_UICR = [(0x10001200, 0x15), (0x10001204, 0x15), (0x1000120C, 0xFFFFFFFE)]
OPEN_APP_SYMBOL = "battletime_firmware::restart::OPEN_APP"
KEPT_CLOCK_SYMBOL = "battletime_firmware::clock::KEPT_CLOCK"
KEPT_CLOCK_MARKER = 0x454D_4954

# The settings file: its format byte, then records of a setting's number and its value.
SETTINGS_FILE_FORMAT = 1
SCREEN_TIMEOUT_SECONDS = (4, 120)
USES_24_HOUR_CLOCK = (0, 1)

# Where to touch, in screen pixels, as the harness's pictures of these screens show them.
LAUNCHER_FIRST_ROW = (120, 40)
DATE_AND_TIME_ROW = (120, 208)
YEAR_BELOW_THE_CHOSEN = (190, 160)
HOUR_BELOW_THE_CHOSEN = (85, 133)
SET_BUTTON = (120, 205)
DRAGS_TO_THE_LIST_END = 20
DRAG_STEP_PIXELS = 20
SECONDS_BETWEEN_DRAG_STEPS = 0.03

SECONDS_TO_SETTLE = 1.0
SECONDS_ALLOWED_FOR_THE_STEPS = 30


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
    """Compiles the face, the launcher, Settings and the shared library, and writes the flash image; gives the flash and the library."""
    run(["cargo", "build", "-q", "-p", "battletime-compiler"])
    compiler = str(BATTLETIME / "target" / "debug" / "battletime-compiler")
    packages = []
    for package_id, source in [(FACE_ID, "faces/digital/face.tsx"), (LAUNCHER_ID, "apps/launcher/app.tsx"),
                               (SETTINGS_ID, "apps/settings/app.tsx")]:
        image = folder / f"{package_id}.ntapp"
        run([compiler, source, "-o", str(image)])
        packages.append(f"{package_id}={image}")
    library = folder / "library.bin"
    run([compiler, "library", "-o", str(library)])
    settings = folder / "settings.bin"
    records = b"".join(struct.pack("<Bi", number, value) for number, value in [SCREEN_TIMEOUT_SECONDS, USES_24_HOUR_CLOCK])
    settings.write_bytes(bytes([SETTINGS_FILE_FORMAT]) + records)
    flash = folder / "flash.bin"
    run(["cargo", "run", "-q", "-p", "battletime-packages", "--example", "filesystem_use", "--",
         "--no-theme", "--image", str(flash), *packages, f"/settings/system={settings}"])
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

    def tap(self, point):
        self.command("touch Tap {} {}".format(*point))
        self.wait_emulated(SECONDS_TO_SETTLE)

    def drag_up(self, from_y, to_y, x=120):
        """A finger pressed, moved up in steps and lifted, as scrolling a list takes."""
        self.command(f"touch MoveTo {x} {from_y}")
        self.command("touch Press")
        for y in range(from_y, to_y - 1, -DRAG_STEP_PIXELS):
            self.command(f"touch MoveTo {x} {y}")
            self.wait_emulated(SECONDS_BETWEEN_DRAG_STEPS)
        self.command("touch Release")
        self.wait_emulated(SECONDS_TO_SETTLE)

    def swipe(self, direction):
        self.command(f'touch Swipe "{direction}"')
        self.wait_emulated(SECONDS_TO_SETTLE)

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


def kept_utc_in_milliseconds(engine, address):
    """The UTC the firmware last kept for a reset, or None while it keeps no whole clock."""
    marker, utc_low, utc_high, _zone, _check = struct.unpack("<5I", engine.read_bytes(address, 20))
    return (utc_high << 32 | utc_low) if marker == KEPT_CLOCK_MARKER else None


def expected_after_the_steps(before_in_milliseconds):
    """The local time the steps choose: a year and an hour on, the same minute, at 00 seconds."""
    before = datetime.fromtimestamp(before_in_milliseconds / 1000, timezone.utc)
    last_day = calendar.monthrange(before.year + 1, before.month)[1]
    chosen = before.replace(year=before.year + 1, day=min(before.day, last_day), hour=(before.hour + 1) % 24,
                            second=0, microsecond=0)
    return int(chosen.timestamp() * 1000)


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    firmware = Path(sys.argv[1]).resolve()
    picture_folder = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
    open_app_address, open_app_size = symbol(firmware, OPEN_APP_SYMBOL)
    kept_clock_address, _ = symbol(firmware, KEPT_CLOCK_SYMBOL)
    emulator = emulator_folder()
    results = []

    with tempfile.TemporaryDirectory(prefix="check-set-clock-") as temporary:
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
        script = folder / "set_clock.resc"
        script.write_text("\n".join(lines) + "\n")

        engine = Engine(emulator, script)

        def open_app():
            return engine.read_bytes(open_app_address, open_app_size).split(b"\0")[0].decode(errors="replace")

        def wait_for_app(app_id, seconds):
            end = time.monotonic() + seconds
            while open_app() != app_id and time.monotonic() < end:
                time.sleep(0.2)
            return open_app() == app_id

        def picture(name):
            if picture_folder is not None:
                picture_folder.mkdir(parents=True, exist_ok=True)
                engine.command(f"lcd SaveScreenshot @{picture_folder / name}.png")

        try:
            if not wait_for_app(FACE_ID, 180):
                sys.exit("the face never opened")
            engine.wait_emulated(SECONDS_TO_SETTLE)
            picture("1-face-before")

            engine.swipe("up")
            results.append(check("the launcher opens", wait_for_app(LAUNCHER_ID, 30), open_app()))
            picture("2-launcher")
            engine.wait_emulated(SECONDS_TO_SETTLE)
            engine.tap(LAUNCHER_FIRST_ROW)
            results.append(check("Settings opens", wait_for_app(SETTINGS_ID, 30), open_app()))
            picture("3-settings")
            for _ in range(DRAGS_TO_THE_LIST_END):
                engine.drag_up(220, 20)
            picture("4-list-end")

            before = kept_utc_in_milliseconds(engine, kept_clock_address)
            engine.tap(DATE_AND_TIME_ROW)
            picture("5-date")
            engine.tap(YEAR_BELOW_THE_CHOSEN)
            engine.swipe("left")
            engine.tap(HOUR_BELOW_THE_CHOSEN)
            picture("6-time")
            seconds_before = engine.emulated_seconds()
            engine.tap(SET_BUTTON)
            picture("7-back-in-the-list")
            after = kept_utc_in_milliseconds(engine, kept_clock_address)
            seconds_after = engine.emulated_seconds()

            expected = expected_after_the_steps(before)
            counted_on = after - expected
            shown = datetime.fromtimestamp(after / 1000, timezone.utc).isoformat(timespec="seconds")
            results.append(check("Set moves the kept clock to the chosen time",
                                 0 <= counted_on <= (seconds_after - seconds_before + 1) * 1000,
                                 f"{shown}, {counted_on} ms after the chosen time"))

            engine.command("sideButton Hold 0.1")
            engine.wait_emulated(SECONDS_TO_SETTLE)
            engine.command("sideButton Hold 0.1")
            results.append(check("going back reaches the face", wait_for_app(FACE_ID, 30), open_app()))
            engine.wait_emulated(SECONDS_TO_SETTLE)
            picture("8-face-after")
        finally:
            engine.stop()

    sys.exit(0 if all(results) else 1)


main()
