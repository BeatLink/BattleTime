"""Checks in the PineTime emulator that a setting the wearer changes reaches the external flash through the shell's write queue.

It installs a face, the launcher and the Settings app on the emulated external flash, opens Settings
from the launcher, taps the Always on switch and presses the button to go back to the face, which
saves the settings. The shell queues that write and the storage task carries it out, so a moment
later the settings file on the emulated flash must say the screen stays on.

    python3 -I tools/check_shell_writes.py FIRMWARE_ELF [SCREENSHOTS]

Run it inside `nix develop`, with the firmware built (`cd battletime/firmware && cargo build
--release`). SCREENSHOTS, when given, is a folder that gets the screen at each step. It finds the
emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator folder beside the main checkout, and takes
about a minute.
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

FACE_ID = "analog"
LAUNCHER_ID = "launcher"
SETTINGS_ID = "settings"
PACKAGES = [
    (FACE_ID, f"faces/{FACE_ID}/face.tsx"),
    (LAUNCHER_ID, f"apps/{LAUNCHER_ID}/app.tsx"),
    (SETTINGS_ID, f"apps/{SETTINGS_ID}/app.tsx"),
]
FIRMWARE_VECTOR_TABLE = 0x8100
IMAGE_HEADER_SIZE = 256
FIRST_START_UICR = [(0x10001200, 0x15), (0x10001204, 0x15), (0x1000120C, 0xFFFFFFFE)]
OPEN_APP_SYMBOL = "battletime_firmware::restart::OPEN_APP"

# The settings file: its format byte, then records of a setting's number and its value.
SETTINGS_PATH = "/settings/system"
SETTINGS_FILE_FORMAT = 1
ALWAYS_ON_SCREEN_SETTING = 5

SECONDS_TO_OPEN = 120
SECONDS_TO_SWITCH = 10
# Long enough for the storage task to write the queued file, which takes a few milliseconds.
SECONDS_TO_WRITE = 2

# ============================================================================
# Where to touch, from the launcher and Settings snapshots
# ============================================================================

# The launcher's first row, the Settings app, the only app listed.
FIRST_LAUNCHER_ROW = (120, 34)
# The Always on switch, the third row of Settings.
ALWAYS_ON_SWITCH = (190, 140)


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
    """Compiles the face, the launcher, Settings and the shared library; gives the flash image and the library."""
    run(["cargo", "build", "-q", "-p", "battletime-compiler"])
    compiler = str(BATTLETIME / "target" / "debug" / "battletime-compiler")
    pairs = []
    for package_id, source in PACKAGES:
        image = folder / f"{package_id}.ntapp"
        run([compiler, source, "-o", str(image)])
        pairs.append(f"{package_id}={image}")
    library = folder / "library.bin"
    run([compiler, "library", "-o", str(library)])
    flash = folder / "flash.bin"
    filesystem_use(["--no-theme", "--image", str(flash), *pairs])
    return flash, library


def filesystem_use(arguments):
    return run(["cargo", "run", "-q", "-p", "battletime-packages", "--example", "filesystem_use", "--", *arguments])


def library_lines(library, folder):
    """Emulator lines that load the image header and the library behind the ELF, where the firmware finds it."""
    header = folder / "header.bin"
    address = run([sys.executable, "-I", str(REPOSITORY / "tools" / "image_header.py"),
                   "--library-beside-elf", str(library), str(header)]).strip()
    return [
        f"sysbus LoadBinary @{header} {FIRMWARE_VECTOR_TABLE - IMAGE_HEADER_SIZE:#x}",
        f"sysbus LoadBinary @{library} {address}",
    ]


def setting_in(settings_hex, number):
    """The value a settings file, in hex, holds for a setting, or None when it holds none."""
    if settings_hex == "missing":
        return None
    settings = bytes.fromhex(settings_hex)
    if not settings or settings[0] != SETTINGS_FILE_FORMAT:
        return None
    for start in range(1, len(settings) - 4, 5):
        record_number, value = struct.unpack_from("<Bi", settings, start)
        if record_number == number:
            return value
    return None


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
        end = self.emulated_seconds() + seconds
        while self.emulated_seconds() < end:
            time.sleep(0.05)

    def read_bytes(self, address, size):
        reply = self.command(f"sysbus ReadBytes {address:#x} {size:#x}")
        return bytes(int(byte, 16) for byte in re.findall(r"0x([0-9A-Fa-f]{2})\b", reply))

    def tap(self, point):
        self.command(f"touch Tap {point[0]} {point[1]}")
        self.wait_emulated(0.5)

    def swipe(self, direction):
        self.command(f"touch Swipe {direction}")
        self.wait_emulated(0.6)

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
# Moving around the watch
# ============================================================================

class Watch:
    """The emulated watch, with the open app read from the firmware's crash record name."""

    def __init__(self, engine, open_app_symbol, screenshots):
        self.engine = engine
        self.open_app_address, self.open_app_size = open_app_symbol
        self.screenshots = screenshots
        self.shots = 0

    def open_app(self):
        return self.engine.read_bytes(self.open_app_address, self.open_app_size).split(b"\0")[0].decode(errors="replace")

    def wait_for_open_app(self, wanted, seconds):
        end = self.engine.emulated_seconds() + seconds
        while self.open_app() != wanted and self.engine.emulated_seconds() < end:
            time.sleep(0.1)
        return self.open_app()

    def until_open(self, wanted, gesture, tries=4):
        """Repeats a gesture until the app opens, since one made while a transition draws can be missed."""
        for _ in range(tries):
            gesture()
            if self.wait_for_open_app(wanted, SECONDS_TO_SWITCH / 5) == wanted:
                break
        return self.open_app()

    def screenshot(self, name):
        if self.screenshots:
            self.shots += 1
            self.engine.command(f"lcd SaveScreenshot @{self.screenshots / f'{self.shots:02}-{name}.png'}")


# ============================================================================
# The check
# ============================================================================

def check(name, passed, detail):
    print(f"{'ok  ' if passed else 'FAIL'}  {name}: {detail}")
    return passed


def start_lines(emulator, firmware, library, flash, folder):
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
    return lines


def check_always_on_is_saved(emulator, firmware, library, flash, folder, screenshots):
    script = folder / "shell-writes.resc"
    script.write_text("\n".join(start_lines(emulator, firmware, library, flash, folder)) + "\n")
    engine = Engine(emulator, script)
    watch = Watch(engine, symbol(firmware, OPEN_APP_SYMBOL), screenshots)
    saved = folder / "saved.bin"
    try:
        results = [check("the face opens", watch.wait_for_open_app(FACE_ID, SECONDS_TO_OPEN) == FACE_ID,
                         f"open app {watch.open_app() or 'none'}")]
        watch.until_open(LAUNCHER_ID, lambda: engine.swipe("Up"))
        opened = watch.until_open(SETTINGS_ID, lambda: engine.tap(FIRST_LAUNCHER_ROW))
        results.append(check("Settings opens from the launcher", opened == SETTINGS_ID, f"open app {opened or 'none'}"))
        engine.wait_emulated(1)
        watch.screenshot("settings")
        engine.tap(ALWAYS_ON_SWITCH)
        watch.screenshot("always-on")
        back = watch.until_open(FACE_ID, lambda: engine.command("sideButton PressAndRelease"))
        results.append(check("the button goes back to the face", back == FACE_ID, f"open app {back or 'none'}"))
        engine.wait_emulated(SECONDS_TO_WRITE)
        engine.command(f"externalFlash SaveBinary @{saved}")
    finally:
        engine.stop()

    before = setting_in(filesystem_use(["--read", str(flash), SETTINGS_PATH]).strip(), ALWAYS_ON_SCREEN_SETTING)
    after = setting_in(filesystem_use(["--read", str(saved), SETTINGS_PATH]).strip(), ALWAYS_ON_SCREEN_SETTING)
    results.append(check("the saved settings keep the screen on", before != 1 and after == 1,
                         f"always_on_screen {before} before, {after} after"))
    return results


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    firmware = Path(sys.argv[1]).resolve()
    screenshots = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
    if screenshots:
        screenshots.mkdir(parents=True, exist_ok=True)
    emulator = emulator_folder()

    with tempfile.TemporaryDirectory(prefix="check-shell-writes-") as temporary:
        folder = Path(temporary)
        flash, library = build_install(folder)
        results = check_always_on_is_saved(emulator, firmware, library, flash, folder, screenshots)

    sys.exit(0 if all(results) else 1)


main()
