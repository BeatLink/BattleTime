"""Checks in the PineTime emulator that moving the recovery slot over the Nordic UART copies its image whole.

It puts a recovery image, made here from random bytes with a real header, in the default recovery
slot of the emulated external flash, starts the firmware with the emulator's Bluetooth bridge,
connects as a phone and sends `{"t":"slots","recovery":[start,size]}` for a slot in the free space
below it. The firmware erases and writes the new slot through `SlotFlash`, waiting on the timer, so
the copy must read back byte for byte at the new start, the old copy must stay, and the reply must
name the new slot and say the watch restarts.

    python3 -I tools/check_slot_move.py FIRMWARE_ELF

Run it inside `nix develop`, with the firmware built (`cd battletime/firmware && cargo build --release`).
It finds the emulator as tools/check_call.py does and takes about a minute.
"""

import json
import random
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

# The helpers sit beside this script; importing them should leave no bytecode in the repository.
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_call  # noqa: E402
import packages_over_hci as hci  # noqa: E402

# ============================================================================
# The slots
# ============================================================================

# The default recovery slot, and the new one in the free space between it and the update slot.
OLD_RECOVERY_START = 0xC6000
NEW_RECOVERY_START = 0x79000
NEW_RECOVERY_SIZE = 0x20000
# Enough code that the copy takes several sector erases and many page programs.
RECOVERY_CODE_BYTES = 20_000
# The image tool wants the code to start with a stack pointer in RAM and a reset vector.
STACK_TOP = 0x20010000

NORDIC_UART_TRANSMIT_UUID = hci.uuid_bytes("6e400003-b5a3-f393-e0a9-e50e24dcca9e")
SLOTS_LINE = json.dumps({"t": "slots", "recovery": [NEW_RECOVERY_START, NEW_RECOVERY_SIZE]}, separators=(",", ":"))

BRIDGE_READY_SECONDS = 1
# The face opens, and the firmware starts advertising, a few emulated seconds after start.
SECONDS_TO_ADVERTISE = 5
# The reply goes out 200 ms before the restart; the flash is saved after that.
SECONDS_AFTER_REPLY = 1


def recovery_image(folder):
    """A recovery image with a valid header around random code, as the firmware checks before copying it."""
    code = folder / "recovery.code.bin"
    vector_table = struct.pack("<II", STACK_TOP, check_call.FIRMWARE_VECTOR_TABLE + 0x101)
    code.write_bytes(vector_table + random.Random(7).randbytes(RECOVERY_CODE_BYTES))
    image = folder / "recovery.bin"
    subprocess.run([sys.executable, "-I", str(check_call.REPOSITORY / "tools" / "image_header.py"),
                    "--type", "recovery", "--build-time", "0", str(code), str(image)], check=True,
                   capture_output=True)
    return image.read_bytes(), image


def start_engine(emulator, firmware, library, flash, recovery, folder):
    lines = [f"include @{emulator}/scripts/pinetime.resc"]
    lines += [f"sysbus WriteDoubleWord {address:#x} {value:#x}" for address, value in check_call.FIRST_START_UICR]
    lines += [
        f"sysbus LoadELF @{firmware}",
        *check_call.library_lines(library, folder),
        f"externalFlashMemory LoadBinary @{flash} 0",
        f"externalFlashMemory LoadBinary @{recovery} {OLD_RECOVERY_START:#x}",
        f"cpu VectorTableOffset {check_call.FIRMWARE_VECTOR_TABLE:#x}",
        f"cpu SP `sysbus ReadDoubleWord {check_call.FIRMWARE_VECTOR_TABLE:#x}`",
        f"cpu PC `sysbus ReadDoubleWord {check_call.FIRMWARE_VECTOR_TABLE + 4:#x}`",
        "start",
    ]
    script = folder / "slots.resc"
    script.write_text("\n".join(lines) + "\n")
    link = folder / "slots-hci"
    engine = check_call.Engine(emulator, script, link)
    try:
        if not link.exists():
            sys.exit("the engine never opened its Bluetooth bridge")
        # The bridge's port refuses writes until the emulation is running.
        while engine.emulated_seconds() < BRIDGE_READY_SECONDS:
            time.sleep(0.1)
        port = hci.HciPort(str(link))
        port.start_controller()
    except BaseException:
        engine.stop()
        raise
    return engine, port


def check_recovery_move(emulator, firmware, library, flash, folder):
    image, recovery = recovery_image(folder)
    saved = folder / "saved.bin"
    engine, port = start_engine(emulator, firmware, library, flash, recovery, folder)
    try:
        while engine.emulated_seconds() < SECONDS_TO_ADVERTISE:
            time.sleep(0.1)
        watch = hci.Watch.connect(port)
        receive = watch.characteristic_handle(hci.NORDIC_UART_RECEIVE_UUID)
        watch.subscribe(watch.characteristic_handle(NORDIC_UART_TRANSMIT_UUID))
        reply = watch.run_command(receive, SLOTS_LINE)
        replied = engine.emulated_seconds()
        while engine.emulated_seconds() < replied + SECONDS_AFTER_REPLY:
            time.sleep(0.1)
        engine.command(f"externalFlash SaveBinary @{saved}")
    finally:
        engine.stop()

    flash_after = saved.read_bytes()
    report = json.loads(reply[reply.index("{"):]) if "{" in reply else {}
    return [
        check_call.check("the reply names the new slot and restarts",
                         report.get("recovery") == [NEW_RECOVERY_START, NEW_RECOVERY_SIZE]
                         and report.get("restart") is True, reply),
        check_call.check("the image reads back whole in the new slot",
                         flash_after[NEW_RECOVERY_START:NEW_RECOVERY_START + len(image)] == image,
                         f"{len(image)} bytes at {NEW_RECOVERY_START:#x}"),
        check_call.check("the old copy stays",
                         flash_after[OLD_RECOVERY_START:OLD_RECOVERY_START + len(image)] == image,
                         f"{len(image)} bytes at {OLD_RECOVERY_START:#x}"),
    ]


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    firmware = Path(sys.argv[1]).resolve()
    emulator = check_call.emulator_folder()
    with tempfile.TemporaryDirectory(prefix="check-slot-move-") as temporary:
        folder = Path(temporary)
        pairs, library = check_call.compile_install(folder)
        flash = check_call.write_flash(folder / "flash.bin", pairs)
        results = check_recovery_move(emulator, firmware, library, flash, folder)
    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
