"""Runs the bench firmware in the PineTime emulator and prints what reading the external flash costs beside drawing.

The bench firmware (`cargo build --release --features bench` in battletime/firmware) runs
src/bench.rs in place of the shell. This puts a 16 KB file and a small record on the emulated
external flash for its store reads, starts it, waits until its `RESULTS` say it has finished and
prints them as a table, cycles and microseconds at 64 MHz.

    python3 -I tools/bench_external_flash.py FIRMWARE_ELF
    python3 -I tools/bench_external_flash.py --diagnostics < DUMP

The second form prints the results a bench build on the watch sends after its counters: DUMP is
the diagnostics characteristic (6e540002-0000-4000-8000-4e656f54696d) as hex bytes, such as
bluetoothctl's `read` prints, read once the watch has run for about a minute. For its store reads
to be real, put a 16 KB `/bench/data` on the watch first, which `battletime files put` can do.

Run it inside `nix develop`. It finds the emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator
folder beside the main checkout. The emulator charges the SPI bus 1 µs a byte with no gaps between
pieces and fetches from RAM for free, so the watch's own numbers will be slower.
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
# The bench's results
# ============================================================================

RESULTS_SYMBOL = "battletime_firmware::bench::RESULTS"
FINISHED = 0xBE7C0001
CYCLES_PER_MICROSECOND = 64
SECONDS_TO_FINISH = 600

# In the order of `bench::Measure`, after its State word.
MEASURES = [
    "raw read 32 B (mean)", "raw read 32 B (most)",
    "raw read 256 B (mean)", "raw read 256 B (most)",
    "raw read 1 KB (mean)", "raw read 1 KB (most)",
    "raw read 4 KB (mean)", "raw read 4 KB (most)",
    "store read 32 B (mean)", "store read 32 B (most)",
    "store read 256 B (mean)", "store read 256 B (most)",
    "store read 1 KB (mean)", "store read 1 KB (most)",
    "store read 4 KB (mean)", "store read 4 KB (most)",
    "record read (mean)", "record read (most)",
    "frame, free drawing", "frame, free drawing, 64 B read every strip",
    "frame, free drawing, 256 B read every strip", "frame, free drawing, 1920 B read every strip",
    "frame, 0.5 ms drawing per strip", "frame, busy, 64 B read every strip",
    "frame, busy, 256 B read on 8 strips", "frame, busy, 256 B read every strip",
    "frame, busy, 1920 B read every strip", "frame, busy, 4 KB read in the background",
    "small loop from flash", "small loop from RAM", "small loop from RAM's code alias",
    "large loop from flash", "large loop from RAM", "large loop from RAM's code alias",
    "large loop flash cache hits (count)", "large loop flash cache misses (count)",
    "bytes the 4 KB store read found (count)",
    "open-file read 32 B (mean)", "open-file read 32 B (most)",
    "open-file read 256 B (mean)", "open-file read 256 B (most)",
    "open-file read 1 KB (mean)", "open-file read 1 KB (most)",
    "open-file read 4 KB (mean)", "open-file read 4 KB (most)",
    "open-file reads match the store (count)",
    "longest strip wait, busy, 4 KB background read",
    "sector erase, left alone",
    "erase suspend: asked to pause until idle",
    "erase suspend: status register 2, top bit set if paused (count)",
    "erase suspend: read while paused matches (count)",
    "erase suspend: resumed until the erase ended",
    "erase suspend: sector erased at the end (count)",
]

# ============================================================================
# The install
# ============================================================================

FIRMWARE_VECTOR_TABLE = 0x8100
IMAGE_HEADER_SIZE = 256
# The UICR words embassy-nrf writes on the first start, so the emulated chip is not reset.
FIRST_START_UICR = [(0x10001200, 0x15), (0x10001204, 0x15), (0x1000120C, 0xFFFFFFFE)]
DATA_BYTES = 16 * 1024
RECORD_BYTES = 600


def run(command, folder=BATTLETIME):
    return subprocess.run(command, capture_output=True, text=True, cwd=folder, check=True).stdout


def symbol(firmware, name):
    """The address and size of a symbol in the firmware."""
    for line in run(["nm", "-S", "-C", str(firmware)]).splitlines():
        parts = line.split(None, 3)
        if len(parts) == 4 and parts[3] == name:
            return int(parts[0], 16), int(parts[1], 16)
    sys.exit(f"{firmware} has no {name}; build it with --features bench")


def emulator_folder():
    if "PINETIME_EMULATOR" in os.environ:
        return Path(os.environ["PINETIME_EMULATOR"])
    common = run(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"]).strip()
    return Path(common).parent.parent / "PineTimeEmulator"


def build_library(folder):
    """Compiles the shared library, whose font the status page draws with."""
    run(["cargo", "build", "-q", "-p", "battletime-compiler"])
    compiler = str(BATTLETIME / "target" / "debug" / "battletime-compiler")
    library = folder / "library.bin"
    run([compiler, "library", "-o", str(library)])
    return library


def build_flash(folder):
    """Writes a flash image holding the bench's 16 KB file and its record."""
    data = folder / "data.bin"
    data.write_bytes(bytes(index * 7 & 0xFF for index in range(DATA_BYTES)))
    record = folder / "record.bin"
    record.write_bytes(bytes(index & 0xFF for index in range(RECORD_BYTES)))
    flash = folder / "flash.bin"
    run(["cargo", "run", "-q", "-p", "battletime-packages", "--example", "filesystem_use", "--",
         "--no-theme", "--image", str(flash), f"/bench/data={data}", f"/bench/record={record}"])
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

    def read_bytes(self, address, size):
        return bytes(int(byte, 16) for byte in re.findall(r"0x([0-9A-Fa-f]{2})\b", self.command(f"sysbus ReadBytes {address:#x} {size:#x}")))

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
# The run
# ============================================================================

def bench_results(emulator, firmware, folder, library, flash):
    """Starts the bench firmware and gives its result words once it has finished, or None if it never does."""
    results_address, size = symbol(firmware, RESULTS_SYMBOL)
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
    script = folder / "bench.resc"
    script.write_text("\n".join(lines) + "\n")
    engine = Engine(emulator, script)
    try:
        end = time.monotonic() + SECONDS_TO_FINISH
        while time.monotonic() < end:
            words = struct.unpack(f"<{size // 4}I", engine.read_bytes(results_address, size))
            if words[0] == FINISHED:
                return words[1:]
            time.sleep(1)
        return None
    finally:
        engine.stop()


def print_table(words):
    width = max(len(name) for name in MEASURES)
    print(f"{'measure':<{width}}  {'cycles':>10}  {'µs':>10}")
    for name, value in zip(MEASURES, words):
        if name.endswith("(count)"):
            print(f"{name:<{width}}  {value:>10}  {'':>10}")
        else:
            print(f"{name:<{width}}  {value:>10}  {value / CYCLES_PER_MICROSECOND:>10.1f}")


def words_from_dump(text):
    """The bench's words from a hex dump of the diagnostics value, which ends with them."""
    dump = bytes(int(byte, 16) for byte in re.findall(r"\b([0-9A-Fa-f]{2})\b", text))
    count = len(MEASURES) + 1
    if len(dump) < count * 4:
        sys.exit(f"the dump holds {len(dump)} bytes, fewer than the bench's {count * 4}")
    words = struct.unpack(f"<{count}I", dump[-count * 4:])
    if words[0] != FINISHED:
        sys.exit("the bench has not finished yet; read the value again in a while")
    return words[1:]


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    if sys.argv[1] == "--diagnostics":
        print_table(words_from_dump(sys.stdin.read()))
        return
    firmware = Path(sys.argv[1]).resolve()
    emulator = emulator_folder()
    with tempfile.TemporaryDirectory(prefix="bench-flash-") as temporary:
        folder = Path(temporary)
        library = build_library(folder)
        flash = build_flash(folder)
        words = bench_results(emulator, firmware, folder, library, flash)
    if words is None:
        sys.exit("the bench never finished")
    print_table(words)


main()
