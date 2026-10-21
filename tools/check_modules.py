"""Checks native modules end to end in the PineTime emulator: the verifier module checks apps, and the test module is placed, called and its crash recorded.

The emulated external flash holds a face, the call app and every module, as `filesystem_use`
writes it, and no record of verified images, so the face opening at start shows the verifier
module ran. The script starts the firmware with the emulator's Bluetooth bridge and connects
through `tools/packages_over_hci.py`, without BlueZ. Installing the call app again must answer
with its need once the shell verified it, and the face must be back afterwards. Call 0 must answer 385,
which it reaches only through its heap, its data and function addresses the loader moved; an
unknown call must answer -1, and a missing module an error. Call 2 then runs an undefined
instruction, and the crash record in RAM must name a HardFault inside `mod:test`.

    python3 -I tools/check_modules.py FIRMWARE_ELF

Run it inside `nix develop`, with the firmware built (`cd battletime/firmware && cargo build --release`).
It finds the emulator as `tools/check_call.py` does and takes about a minute.
"""

import struct
import sys
import tempfile
import time
from pathlib import Path

# The other tools sit beside this script; importing them should leave no bytecode in the repository.
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_call  # noqa: E402
import packages_over_hci  # noqa: E402

CRASH_RECORD_SYMBOL = "battletime_firmware::restart::CRASH_RECORD"
RECORD_MAGIC = 0x48535243
HARD_FAULT = 2
OPEN_APP_AT = 4 * (2 + 8)
SECONDS_TO_CRASH = 10


def call_line(name, call):
    return f'{{"c":"module","name":"{name}","call":{call}}}'


def read_crash(engine, address):
    record = engine.read_bytes(address, OPEN_APP_AT + 16)
    magic, kind = struct.unpack_from("<II", record)
    open_app = record[OPEN_APP_AT:].split(b"\0")[0].decode(errors="replace")
    return magic, kind, open_app


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    firmware = Path(sys.argv[1]).resolve()
    crash_address, _ = check_call.symbol(firmware, CRASH_RECORD_SYMBOL)
    open_app_address, open_app_size = check_call.symbol(firmware, check_call.OPEN_APP_SYMBOL)
    emulator = check_call.emulator_folder()
    results = []

    with tempfile.TemporaryDirectory(prefix="check-modules-") as temporary:
        folder = Path(temporary)
        pairs, library = check_call.compile_install(folder)
        flash = check_call.write_flash(folder / "flash.bin", pairs)
        lines = [f"include @{emulator}/scripts/pinetime.resc"]
        lines += [f"sysbus WriteDoubleWord {address:#x} {value:#x}" for address, value in check_call.FIRST_START_UICR]
        lines += [
            f"sysbus LoadELF @{firmware}",
            *check_call.library_lines(library, folder),
            f"externalFlashMemory LoadBinary @{flash} 0",
            f"cpu VectorTableOffset {check_call.FIRMWARE_VECTOR_TABLE:#x}",
            f"cpu SP `sysbus ReadDoubleWord {check_call.FIRMWARE_VECTOR_TABLE:#x}`",
            f"cpu PC `sysbus ReadDoubleWord {check_call.FIRMWARE_VECTOR_TABLE + 4:#x}`",
            "start",
        ]
        script = folder / "modules.resc"
        script.write_text("\n".join(lines) + "\n")
        link = folder / "hci"

        engine = check_call.Engine(emulator, script, link)
        try:
            if not link.exists():
                sys.exit("the engine never opened its Bluetooth bridge")
            face = check_call.wait_for_open_app(engine, open_app_address, open_app_size,
                                                lambda current: current == check_call.FACE_ID, seconds=60)
            results.append(check_call.check("the face opens once the verifier module checked it",
                                            face == check_call.FACE_ID, f"open app {face!r}"))
            port = packages_over_hci.HciPort(str(link))
            port.start_controller()
            watch = packages_over_hci.Watch.connect(port)
            packages = watch.open_packages()

            try:
                reply = watch.run_command(packages, f'{{"c":"install","id":"{check_call.CALL_APP_ID}"}}')
            except packages_over_hci.Disconnected as error:
                magic, kind, open_app = read_crash(engine, crash_address)
                crashed = f", crash kind {kind} in {open_app!r}" if magic == RECORD_MAGIC else ""
                crashed += f", open app {check_call.open_app(engine, open_app_address, open_app_size)!r}"
                crashed += f", pc {engine.command('cpu PC')}"
                check_call.check("an install answers once the shell verified the app", False, f"{error}{crashed}")
                sys.exit(1)
            results.append(check_call.check("an install answers once the shell verified the app",
                                            '"ok":1' in reply and '"need"' in reply, reply))
            face = check_call.wait_for_open_app(engine, open_app_address, open_app_size,
                                                lambda current: current == check_call.FACE_ID)
            results.append(check_call.check("the face is back after the install", face == check_call.FACE_ID,
                                            f"open app {face!r}"))

            started = time.monotonic()
            reply = watch.run_command(packages, call_line("test", 0))
            results.append(check_call.check("call 0 answers 385", '"r":385' in reply,
                                            f"{reply} in {time.monotonic() - started:.1f} s"))
            reply = watch.run_command(packages, call_line("test", 7))
            results.append(check_call.check("an unknown call answers -1", '"r":-1' in reply, reply))
            reply = watch.run_command(packages, call_line("absent", 0))
            results.append(check_call.check("a missing module is an error", '"err"' in reply, reply))

            # The watch halts inside the call, so the write may never be acknowledged.
            try:
                watch.write_line(packages, call_line("test", 2), seconds=SECONDS_TO_CRASH)
            except (packages_over_hci.Disconnected, TimeoutError):
                pass
            end = time.monotonic() + SECONDS_TO_CRASH
            while True:
                magic, kind, open_app = read_crash(engine, crash_address)
                if (magic == RECORD_MAGIC and kind == HARD_FAULT) or time.monotonic() > end:
                    break
                time.sleep(0.2)
            results.append(check_call.check("call 2 faults and the crash names the module",
                                            magic == RECORD_MAGIC and kind == HARD_FAULT and open_app == "mod:test",
                                            f"magic {magic:#x}, kind {kind}, open app {open_app!r}"))
        finally:
            engine.stop()

    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
