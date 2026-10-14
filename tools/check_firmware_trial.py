"""Checks the System app's firmware buttons in the PineTime emulator: Confirm keeps a firmware on trial, and Roll back brings back the one before.

It installs a face, the launcher and the System app on the emulated external flash, puts dist/'s
firmware in the update slot with the recovery firmware standing in for the one before it, and marks
the swap, as an update does. The bootloader swaps the firmware in on trial, and with no phone to
connect it stays on trial. Through the touch panel the check opens the launcher, then the System
app on its system page, and taps one of the two buttons it shows while the firmware is on trial:

- Confirm: the boot state must say booted, and a restart must keep the firmware.
- Roll back: the watch must restart at once, and the bootloader must swap the one before back.

    python3 -I tools/check_firmware_trial.py [DIST [SCREENSHOTS]]

DIST is the folder `tools/build_images.sh` fills, `neotime/dist` unless given; the apps are built
here from the same commit, so they match the shared library inside its firmware. SCREENSHOTS, when
given, is a folder that gets the screen at each step. Run it inside `nix develop`. It finds the
emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator folder beside the main checkout, and
takes about three minutes.
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
NEOTIME = REPOSITORY / "neotime"

# ============================================================================
# The install and the flash layout
# ============================================================================

FACE_ID = "digital"
LAUNCHER_ID = "launcher"
SYSTEM_ID = "system"
PACKAGES = [
    (FACE_ID, "faces/digital/face.tsx"),
    (LAUNCHER_ID, "apps/launcher/app.tsx"),
    (SYSTEM_ID, "apps/system/app.tsx"),
]
OPEN_APP_SYMBOL = "neotime_firmware::restart::OPEN_APP"

# The settings file: its format byte, then a record of the screen timeout's number and 300 seconds, so the screen stays lit.
SETTINGS_FILE = bytes([1, 4]) + struct.pack("<i", 300)

# From neotime/boot/src/lib.rs.
BOOT_STATE = 0x7000
FIRMWARE_START = 0x8000
EXTERNAL_FLASH = 0x60000000
UPDATE_SLOT = EXTERNAL_FLASH + 0x000000

# The word each start of a firmware writes near the end of RAM, by which a restart is seen.
ALIVE_MARKER = 0x2000FFF0
ALIVE_MAGIC = 0x4E454F54

# embassy-boot's first word of the boot state page.
STATE_SWAP = 0xF0F0F0F0
STATE_BOOTED = 0xD0D0D0D0
STATE_REVERTED = 0xC0C0C0C0

# A swap of the whole firmware slot takes about 30 emulated seconds, and the firmware a few more to open its face.
SECONDS_TO_SWAP = 60
SECONDS_TO_SWITCH = 10
# Holding the button this long stops the watchdog being fed, so the watch restarts into the bootloader, and lets go before its menu.
SECONDS_TO_RESTART = 5.5

# ============================================================================
# Where to touch, from the launcher and System snapshots
# ============================================================================

# The launcher's first row, the System app, the only app installed.
FIRST_LAUNCHER_ROW = (120, 34)
# The two buttons side by side on the system page while the firmware is on trial.
CONFIRM_BUTTON = (64, 183)
ROLL_BACK_BUTTON = (176, 183)


def run(command, folder=NEOTIME):
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
    """Compiles the face, the launcher and System, and writes the external flash image holding them and the settings file."""
    run(["cargo", "build", "-q", "-p", "neotime-compiler"])
    compiler = str(NEOTIME / "target" / "debug" / "neotime-compiler")
    pairs = []
    for package_id, source in PACKAGES:
        image = folder / f"{package_id}.ntapp"
        run([compiler, source, "-o", str(image)])
        pairs.append(f"{package_id}={image}")
    settings = folder / "settings"
    settings.write_bytes(SETTINGS_FILE)
    flash = folder / "flash.bin"
    run(["cargo", "run", "-q", "-p", "neotime-packages", "--example", "filesystem_use", "--",
         "--no-theme", "--image", str(flash), *pairs, f"/settings/system={settings}"])
    return flash


def start_lines(emulator, dist, flash):
    """Emulator lines for a watch whose update slot holds dist's firmware, marked to be swapped in over the recovery firmware."""
    return [
        f"include @{emulator}/scripts/pinetime.resc",
        f"externalFlashMemory LoadBinary @{flash} 0",
        f"sysbus LoadBinary @{dist / 'bootloader.bin'} 0x0",
        f"sysbus LoadBinary @{dist / 'recovery.bin'} {FIRMWARE_START:#x}",
        f"sysbus LoadBinary @{dist / 'firmware.bin'} {UPDATE_SLOT:#x}",
        f"sysbus WriteDoubleWord {BOOT_STATE:#x} {STATE_SWAP:#x}",
        "start",
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
        # The first start builds the engine through nix, which can take minutes.
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

    def word(self, address):
        return struct.unpack("<I", self.read_bytes(address, 4))[0]

    def holds(self, address, path):
        """Whether the flash at the address starts with the file's first 4 KB."""
        expected = Path(path).read_bytes()[:4096]
        return self.read_bytes(address, len(expected)) == expected

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
# Getting to the firmware buttons
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

    def open_system_page(self):
        """Opens the launcher from the face, and the System app from its first row."""
        self.until_open(LAUNCHER_ID, lambda: self.engine.swipe("Up"))
        self.screenshot("launcher")
        opened = self.until_open(SYSTEM_ID, lambda: self.engine.tap(FIRST_LAUNCHER_ROW))
        self.engine.wait_emulated(1)
        self.screenshot("system")
        return opened


def check(name, passed, detail):
    print(f"{'ok  ' if passed else 'FAIL'}  {name}: {detail}")
    return passed


def state_name(word):
    return {STATE_SWAP: "on trial", STATE_BOOTED: "booted", STATE_REVERTED: "reverted"}.get(word, f"{word:#x}")


def start_on_trial(emulator, dist, flash, folder, open_app_symbol, screenshots, name):
    """Starts a watch, waits for the bootloader to swap the firmware in and its face to open, and checks it is on trial."""
    script = folder / f"{name}.resc"
    script.write_text("\n".join(start_lines(emulator, dist, flash)) + "\n")
    engine = Engine(emulator, script)
    watch = Watch(engine, open_app_symbol, screenshots / name if screenshots else None)
    if watch.screenshots:
        watch.screenshots.mkdir(parents=True, exist_ok=True)
    face = watch.wait_for_open_app(FACE_ID, SECONDS_TO_SWAP)
    started = check(f"{name}: the update starts on trial",
                    face == FACE_ID and engine.holds(FIRMWARE_START, dist / "firmware.bin")
                    and engine.word(BOOT_STATE) == STATE_SWAP,
                    f"open app {face!r}, boot state {state_name(engine.word(BOOT_STATE))}")
    return engine, watch, started


def check_confirm(emulator, dist, flash, folder, open_app_symbol, screenshots):
    engine, watch, started = start_on_trial(emulator, dist, flash, folder, open_app_symbol, screenshots, "confirm")
    results = [started]
    try:
        opened = watch.open_system_page()
        results.append(check("confirm: System opens from the launcher", opened == SYSTEM_ID, f"open app {opened!r}"))
        engine.tap(CONFIRM_BUTTON)
        engine.wait_emulated(1)
        watch.screenshot("confirmed")
        state = engine.word(BOOT_STATE)
        results.append(check("confirm: the boot state says booted", state == STATE_BOOTED, state_name(state)))

        engine.command(f"sysbus WriteDoubleWord {ALIVE_MARKER:#x} 0")
        engine.command(f"sideButton Hold {SECONDS_TO_RESTART}")
        end = engine.emulated_seconds() + SECONDS_TO_SWAP
        while engine.word(ALIVE_MARKER) != ALIVE_MAGIC and engine.emulated_seconds() < end:
            time.sleep(0.2)
        restarted = engine.word(ALIVE_MARKER) == ALIVE_MAGIC
        kept = engine.holds(FIRMWARE_START, dist / "firmware.bin")
        state = engine.word(BOOT_STATE)
        results.append(check("confirm: a restart keeps the firmware", restarted and kept and state == STATE_BOOTED,
                             f"{'restarted' if restarted else 'never restarted'}, firmware {'kept' if kept else 'replaced'},"
                             f" boot state {state_name(state)}"))
    finally:
        engine.stop()
    return results


def check_roll_back(emulator, dist, flash, folder, open_app_symbol, screenshots):
    engine, watch, started = start_on_trial(emulator, dist, flash, folder, open_app_symbol, screenshots, "roll-back")
    results = [started]
    try:
        opened = watch.open_system_page()
        results.append(check("roll back: System opens from the launcher", opened == SYSTEM_ID, f"open app {opened!r}"))
        engine.tap(ROLL_BACK_BUTTON)
        end = engine.emulated_seconds() + SECONDS_TO_SWAP
        while engine.emulated_seconds() < end and not (
            engine.word(BOOT_STATE) == STATE_REVERTED and engine.holds(FIRMWARE_START, dist / "recovery.bin")
        ):
            time.sleep(0.5)
        watch.screenshot("rolled-back")
        restored = engine.holds(FIRMWARE_START, dist / "recovery.bin")
        state = engine.word(BOOT_STATE)
        results.append(check("roll back: the bootloader brings back the firmware before",
                             restored and state == STATE_REVERTED,
                             f"firmware before {'restored' if restored else 'missing'}, boot state {state_name(state)}"))
    finally:
        engine.stop()
    return results


def main():
    dist = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else NEOTIME / "dist"
    screenshots = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
    for image in ("bootloader.bin", "firmware.bin", "firmware.elf", "recovery.bin"):
        if not (dist / image).exists():
            sys.exit(f"{dist / image} is missing; run tools/build_images.sh first")
    open_app_symbol = symbol(dist / "firmware.elf", OPEN_APP_SYMBOL)
    emulator = emulator_folder()

    with tempfile.TemporaryDirectory(prefix="check-firmware-trial-") as temporary:
        folder = Path(temporary)
        flash = build_install(folder)
        results = check_confirm(emulator, dist, flash, folder, open_app_symbol, screenshots)
        results += check_roll_back(emulator, dist, flash, folder, open_app_symbol, screenshots)

    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
