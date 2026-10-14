"""Checks in the PineTime emulator that the watch advertises as InfiniTime does, and as InfiniTime for Gadgetbridge.

It starts the firmware with the emulator's Bluetooth bridge and scans it actively through the bridge's
controller, without BlueZ. The watch must advertise fast for its first 30 seconds and then about once
a second, with the flags, the heart rate and DFU services and the transmit power, and its name in the
scan response. A phone then connects, reads the GAP name and appearance, turns the `gadgetbridge_name`
setting on over the Nordic UART and leaves: the watch must advertise fast again, as "InfiniTime", and
give that name in its GAP service. A second start, with the setting saved on the external flash,
must advertise as "InfiniTime" once the settings are read.

    python3 -I tools/check_advertising.py FIRMWARE_ELF

Run it inside `nix develop`, with the firmware built (`cd neotime/firmware && cargo build --release`).
It finds the emulator as tools/check_call.py does and takes about three minutes.
"""

import struct
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
# What the watch should send
# ============================================================================

DFU_SERVICE = hci.uuid_bytes("00001530-1212-efde-1523-785feabcd123")
# Flags, the Heart Rate service, the legacy DFU service and 0 dBm, as InfiniTime advertises.
ADVERTISEMENT = bytes([2, 0x01, 0x06, 3, 0x03, 0x0D, 0x18, 17, 0x07]) + DFU_SERVICE + bytes([2, 0x0A, 0])
# 0x00C2, a smartwatch.
APPEARANCE = (0x00C2).to_bytes(2, "little")
GADGETBRIDGE_NAME_LINE = '{"t":"settings","gadgetbridge_name":true}'
# `/settings/system` with one record: the format byte, then setting 20 (the Gadgetbridge name) at 1.
GADGETBRIDGE_NAME_SETTINGS = bytes([1, 20]) + struct.pack("<i", 1)

# ============================================================================
# Scanning
# ============================================================================

LE_SET_SCAN_PARAMETERS = 0x200B
LE_SET_SCAN_ENABLE = 0x200C
HCI_DISCONNECT = 0x0406
LE_ADVERTISING_REPORT = 0x02
CONNECTABLE_ADVERTISEMENT = 0x00
SCAN_RESPONSE = 0x04
DEVICE_NAME_UUID = 0x2A00
APPEARANCE_UUID = 0x2A01

# Fast advertising is every 20 to 30 ms, slow about once a second; these rates lie well between.
FAST_SECONDS = 30
FAST_AT_LEAST_PER_SECOND = 8
SLOW_AT_MOST_PER_SECOND = 2
SLOW_AT_LEAST_PER_SECOND = 0.5
# How long each rate is counted, in emulated seconds, kept clear of the change at 30 s.
COUNT_SECONDS = 10
# The emulated time after which the bridge's port takes writes.
BRIDGE_READY_SECONDS = 1
# The shell reads the saved settings a few seconds after start.
SETTINGS_READ_SECONDS = 8


def scan(port, engine, until_emulated_seconds):
    """Scans until the emulated clock reaches the time given; gives each report as (emulated seconds, kind, data)."""
    port.write(hci.command(LE_SET_SCAN_PARAMETERS, bytes([1, 0x10, 0, 0x10, 0, 0, 0])))
    port.wait_for_completion(LE_SET_SCAN_PARAMETERS, 10)
    port.write(hci.command(LE_SET_SCAN_ENABLE, bytes([1, 0])))
    port.wait_for_completion(LE_SET_SCAN_ENABLE, 10)
    reports = []
    now = engine.emulated_seconds()
    asked_at = time.monotonic()
    while now < until_emulated_seconds:
        for kind, code, payload in port.packets(0.2):
            if kind == hci.EVENT_PACKET and code == hci.LE_META_EVENT and payload[0] == LE_ADVERTISING_REPORT:
                reports += [(now, *report) for report in advertising_reports(payload)]
        if time.monotonic() - asked_at > 0.2:
            now = engine.emulated_seconds()
            asked_at = time.monotonic()
    port.write(hci.command(LE_SET_SCAN_ENABLE, bytes([0, 0])))
    port.wait_for_completion(LE_SET_SCAN_ENABLE, 10)
    return reports


def advertising_reports(payload):
    """Each report in an LE Advertising Report event as (kind, data)."""
    offset = 2
    for _ in range(payload[1]):
        kind = payload[offset]
        length = payload[offset + 8]
        yield kind, payload[offset + 9:offset + 9 + length]
        offset += 10 + length


def per_second(reports, start, end):
    count = sum(1 for when, kind, _ in reports if kind == CONNECTABLE_ADVERTISEMENT and start <= when < end)
    return count / (end - start)


def scan_response_names(reports):
    return {data[2:2 + data[0] - 1].decode(errors="replace") for _, kind, data in reports
            if kind == SCAN_RESPONSE and len(data) >= 2 and data[1] == 0x09}


def advertisements(reports):
    return {bytes(data) for _, kind, data in reports if kind == CONNECTABLE_ADVERTISEMENT}


def read_by_type(watch, uuid):
    answer = watch.request(bytes([hci.READ_BY_TYPE_REQUEST]) + (1).to_bytes(2, "little")
                           + (0xFFFF).to_bytes(2, "little") + uuid.to_bytes(2, "little"), hci.READ_BY_TYPE_RESPONSE)
    return answer[4:2 + answer[1]]


def disconnect(port, watch):
    port.write(hci.command(HCI_DISCONNECT, watch.handle.to_bytes(2, "little") + bytes([0x13])))
    for kind, code, _ in port.packets(30):
        if kind == hci.EVENT_PACKET and code == hci.DISCONNECTION_COMPLETE:
            return
    raise TimeoutError("the link did not end")


# ============================================================================
# The runs
# ============================================================================

def start_engine(emulator, firmware, library, flash, folder, name):
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
    script = folder / f"{name}.resc"
    script.write_text("\n".join(lines) + "\n")
    link = folder / f"{name}-hci"
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


def check_first_start(emulator, firmware, library, flash, folder):
    results = []
    engine, port = start_engine(emulator, firmware, library, flash, folder, "first")
    try:
        started = engine.emulated_seconds()
        reports = scan(port, engine, started + FAST_SECONDS + COUNT_SECONDS + 5)
        fast = per_second(reports, started + 5, started + 5 + COUNT_SECONDS)
        slow = per_second(reports, started + FAST_SECONDS + 5, started + FAST_SECONDS + 5 + COUNT_SECONDS)
        results.append(check_call.check("fast advertising at first", fast >= FAST_AT_LEAST_PER_SECOND,
                                        f"{fast:.1f} advertisements a second"))
        results.append(check_call.check("about once a second after 30 s",
                                        SLOW_AT_LEAST_PER_SECOND <= slow <= SLOW_AT_MOST_PER_SECOND,
                                        f"{slow:.1f} advertisements a second"))
        sent = advertisements(reports)
        results.append(check_call.check("InfiniTime's advertisement", sent == {ADVERTISEMENT},
                                        ", ".join(data.hex() for data in sent)))
        names = scan_response_names(reports)
        results.append(check_call.check("name in the scan response", names == {"NeoTime"}, f"{names}"))

        watch = hci.Watch.connect(port)
        name = read_by_type(watch, DEVICE_NAME_UUID)
        appearance = read_by_type(watch, APPEARANCE_UUID)
        results.append(check_call.check("GAP name", name == b"NeoTime", f"{name!r}"))
        results.append(check_call.check("GAP appearance 0x00C2", appearance == APPEARANCE, appearance.hex()))
        receive = watch.characteristic_handle(hci.NORDIC_UART_RECEIVE_UUID)
        watch.write_line(receive, GADGETBRIDGE_NAME_LINE)
        disconnect(port, watch)

        left = engine.emulated_seconds()
        reports = scan(port, engine, left + COUNT_SECONDS)
        fast = per_second(reports, left + 1, left + COUNT_SECONDS)
        names = scan_response_names(reports)
        results.append(check_call.check("fast again after the phone leaves", fast >= FAST_AT_LEAST_PER_SECOND,
                                        f"{fast:.1f} advertisements a second"))
        results.append(check_call.check("InfiniTime in the scan response once set", names == {"InfiniTime"},
                                        f"{names}"))
        watch = hci.Watch.connect(port)
        name = read_by_type(watch, DEVICE_NAME_UUID)
        results.append(check_call.check("InfiniTime as the GAP name", name == b"InfiniTime", f"{name!r}"))
    finally:
        engine.stop()
    return results


def check_saved_setting(emulator, firmware, library, flash, folder):
    engine, port = start_engine(emulator, firmware, library, flash, folder, "saved")
    try:
        started = engine.emulated_seconds()
        reports = scan(port, engine, started + SETTINGS_READ_SECONDS + 5)
        names = scan_response_names([report for report in reports if report[0] >= started + SETTINGS_READ_SECONDS])
        return [check_call.check("a saved setting advertises InfiniTime", names == {"InfiniTime"}, f"{names}")]
    finally:
        engine.stop()


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    firmware = Path(sys.argv[1]).resolve()
    emulator = check_call.emulator_folder()
    with tempfile.TemporaryDirectory(prefix="check-advertising-") as temporary:
        folder = Path(temporary)
        pairs, library = check_call.compile_install(folder)
        flash = check_call.write_flash(folder / "flash.bin", pairs)
        settings = folder / "settings"
        settings.write_bytes(GADGETBRIDGE_NAME_SETTINGS)
        saved = check_call.write_flash(folder / "saved.bin", [*pairs, f"/settings/system={settings}"])
        results = check_first_start(emulator, firmware, library, flash, folder)
        results += check_saved_setting(emulator, firmware, library, saved, folder)
    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
