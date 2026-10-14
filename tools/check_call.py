"""Checks an incoming call end to end in the PineTime emulator: the call app opens and the watch rings until the call ends.

It installs a face and the call app on the emulated external flash, starts the firmware with the
emulator's Bluetooth bridge, and connects through `tools/packages_over_hci.py`, without BlueZ. It
writes a Gadgetbridge incoming call line to the Nordic UART, as the phone does, and checks that the
open app becomes `call` and that the motor is still buzzing seconds later, which a single
notification buzz never is. It then writes the end line and checks that the motor stops and the
shell goes back to the face.

    python3 -I tools/check_call.py FIRMWARE_ELF [SCREENSHOT]

SCREENSHOT, when given, saves the ringing screen. Run it inside `nix develop`, with the firmware
built (`cd neotime/firmware && cargo build --release`). It finds the emulator in
$PINETIME_EMULATOR, or in a PineTimeEmulator folder beside the main checkout, and takes about a
minute.
"""

import os
import re
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

# The client sits beside this script; importing it should leave no bytecode in the repository.
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import packages_over_hci  # noqa: E402

REPOSITORY = Path(__file__).resolve().parent.parent
# The watch's crates, apps and build output.
NEOTIME = REPOSITORY / "neotime"

# ============================================================================
# The install and the watch
# ============================================================================

FACE_ID = "digital"
CALL_APP_ID = "call"
PACKAGES = [(FACE_ID, "faces/digital/face.tsx"), (CALL_APP_ID, "apps/call/app.tsx")]
FIRMWARE_VECTOR_TABLE = 0x8100
IMAGE_HEADER_SIZE = 256
# The UICR words embassy-nrf writes on the first start, so the emulated chip is not reset.
FIRST_START_UICR = [(0x10001200, 0x15), (0x10001204, 0x15), (0x1000120C, 0xFFFFFFFE)]
# The id of the app, face or launcher open now, which restart.rs keeps for the crash record.
OPEN_APP_SYMBOL = "neotime_firmware::restart::OPEN_APP"
MOTOR_STATE = "sysbus.gpio0.vibrationMotor State"

INCOMING_LINE = 'GB({"t":"call","cmd":"incoming","name":"Sam","number":"+15550100"})'
END_LINE = 'GB({"t":"call","cmd":"end"})'

# How long the shell may take to open the call app, or to go back, once a line is written.
SECONDS_TO_SWITCH = 10
# The call ring repeats every 2.5 s, so the motor must still be starting this long after it began; a notification buzz lasts 0.28 s.
RINGING_SECONDS = 4
# Once the end line is written the motor may finish the step it is in, then must stay still this long.
SETTLE_SECONDS = 1
STILL_SECONDS = 4
# The engine is asked for the motor this often, in wall clock time, which it keeps pace with.
SAMPLE_SECONDS = 0.02


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


def compile_install(folder):
    """Compiles the packages and the shared library; gives the packages as ID=FILE pairs and the library."""
    run(["cargo", "build", "-q", "-p", "neotime-compiler"])
    compiler = str(NEOTIME / "target" / "debug" / "neotime-compiler")
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


def write_flash(path, pairs):
    """Writes the external flash image holding these ID=FILE packages and a theme that is not all black."""
    run(["cargo", "run", "-q", "-p", "neotime-packages", "--example", "filesystem_use", "--",
         "--no-theme", "--image", str(path), *pairs])
    return path


# ============================================================================
# The engine, running at the wall clock's pace
# ============================================================================

class Engine:
    """A `pinetime-engine` with the Bluetooth bridge, its monitor on a TCP port, sampling the motor from a thread."""

    PROMPT = b"(pinetime) "

    def __init__(self, emulator, script, link):
        port = free_port()
        self.process = subprocess.Popen(
            ["nix", "develop", str(emulator), "-c", "pinetime-engine", "--port", str(port), "--bluetooth", str(link),
             str(script)],
            cwd=emulator, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
        )
        self.connection = self.connect(port)
        self.pending = b""
        self.lock = threading.Lock()
        self.read_reply()
        # Each sample is (emulated seconds, motor running).
        self.samples = []
        self.sampling = True
        self.sampler = threading.Thread(target=self.sample_motor, daemon=True)

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
        with self.lock:
            self.connection.sendall(line.encode() + b"\n")
            reply = self.read_reply()
        if reply.startswith("error:"):
            raise RuntimeError(f"{line}: {reply}")
        return reply

    def emulated_seconds(self):
        return float(self.command("emulation ElapsedTime").split()[0])

    def sample_motor(self):
        while self.sampling:
            is_running = self.command(MOTOR_STATE).lower() == "true"
            self.samples.append((self.emulated_seconds(), is_running))
            time.sleep(SAMPLE_SECONDS)

    def read_bytes(self, address, size):
        return bytes(int(byte, 16) for byte in re.findall(r"0x([0-9A-Fa-f]{2})\b", self.command(f"sysbus ReadBytes {address:#x} {size:#x}")))

    def stop(self):
        self.sampling = False
        if self.sampler.is_alive():
            self.sampler.join(5)
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

def open_app(engine, address, size):
    return engine.read_bytes(address, size).split(b"\0")[0].decode(errors="replace")


def wait_for_open_app(engine, address, size, wanted, seconds=SECONDS_TO_SWITCH):
    """Polls the open app's id until the test holds or the time runs out, and gives the last id read."""
    end = time.monotonic() + seconds
    while True:
        current = open_app(engine, address, size)
        if wanted(current) or time.monotonic() > end:
            return current
        time.sleep(0.1)


def motor_starts(samples, start, stop):
    """The emulated times at which the motor went from still to running between start and stop."""
    starts = []
    was_running = False
    for when, is_running in samples:
        if start <= when < stop:
            if is_running and not was_running:
                starts.append(when)
            was_running = is_running
    return starts


def check(name, passed, detail):
    print(f"{'ok  ' if passed else 'FAIL'}  {name}: {detail}")
    return passed


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    firmware = Path(sys.argv[1]).resolve()
    screenshot = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
    open_app_address, open_app_size = symbol(firmware, OPEN_APP_SYMBOL)
    emulator = emulator_folder()
    results = []

    with tempfile.TemporaryDirectory(prefix="check-call-") as temporary:
        folder = Path(temporary)
        pairs, library = compile_install(folder)
        flash = write_flash(folder / "flash.bin", pairs)
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
        script = folder / "call.resc"
        script.write_text("\n".join(lines) + "\n")
        link = folder / "hci"

        engine = Engine(emulator, script, link)
        try:
            if not link.exists():
                sys.exit("the engine never opened its Bluetooth bridge")
            face = wait_for_open_app(engine, open_app_address, open_app_size, lambda current: current == FACE_ID)
            results.append(check("face open at start", face == FACE_ID, f"open app {face!r}"))

            port = packages_over_hci.HciPort(str(link))
            port.start_controller()
            watch = packages_over_hci.Watch.connect(port)
            receive = watch.characteristic_handle(packages_over_hci.NORDIC_UART_RECEIVE_UUID)
            engine.sampler.start()

            watch.write_line(receive, INCOMING_LINE)
            incoming_at = engine.emulated_seconds()
            current = wait_for_open_app(engine, open_app_address, open_app_size, lambda current: current == CALL_APP_ID)
            results.append(check("call app opens", current == CALL_APP_ID, f"open app {current!r}"))
            while engine.emulated_seconds() < incoming_at + RINGING_SECONDS + 1:
                time.sleep(0.1)
            if screenshot:
                engine.command(f"lcd SaveScreenshot @{screenshot}")
            starts = motor_starts(engine.samples, incoming_at, incoming_at + RINGING_SECONDS + 1)
            rings_on = bool(starts) and starts[-1] - starts[0] >= RINGING_SECONDS - 1.5
            results.append(check("motor rings again and again", rings_on,
                                 "motor starts at " + ", ".join(f"{when - incoming_at:.2f}" for when in starts) + " s"))

            watch.write_line(receive, END_LINE)
            ended_at = engine.emulated_seconds()
            current = wait_for_open_app(engine, open_app_address, open_app_size, lambda current: current != CALL_APP_ID)
            results.append(check("shell goes back", current == FACE_ID, f"open app {current!r}"))
            while engine.emulated_seconds() < ended_at + SETTLE_SECONDS + STILL_SECONDS:
                time.sleep(0.1)
            still = [is_running for when, is_running in engine.samples
                     if ended_at + SETTLE_SECONDS <= when < ended_at + SETTLE_SECONDS + STILL_SECONDS]
            results.append(check("motor stops", bool(still) and not any(still),
                                 f"running in {sum(still)} of {len(still)} samples from {SETTLE_SECONDS} s after the end"))
        finally:
            engine.stop()

    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
