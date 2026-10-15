"""Checks the watch's client for the phone's own services in the PineTime emulator, with this script as the phone.

It installs a face and the call app on the emulated external flash, starts the firmware with the
emulator's Bluetooth bridge, and connects through `tools/packages_over_hci.py`, without BlueZ.
The script then plays a phone that serves the Current Time service (0x1805) and the Alert
Notification service (0x1811), answering the watch's ATT requests from a small attribute table.
It checks that the watch finds both services by itself a few seconds after connecting, sets its
clock from the phone's Current Time and Local Time Information, turns on New Alert notifications
and asks for every category through the control point. It then notifies an SMS alert, which must
buzz the motor, and an incoming call alert, which must open the call app.

    python3 -I tools/check_phone_services.py FIRMWARE_ELF [SCREENSHOT]

SCREENSHOT, when given, saves the call screen the alert opened. Run it inside `nix develop`, with
the firmware built (`cd battletime/firmware && cargo build --release`). It finds the emulator in
$PINETIME_EMULATOR, or in a PineTimeEmulator folder beside the main checkout, and takes about a
minute.
"""

import calendar
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
from packages_over_hci import ACL_PACKET, ATTRIBUTE_CHANNEL, DISCONNECTION_COMPLETE, EVENT_PACKET  # noqa: E402

# ============================================================================
# The phone's attribute table
# ============================================================================

PRIMARY_SERVICE = 0x2800
CHARACTERISTIC = 0x2803
CLIENT_CHARACTERISTIC_CONFIGURATION = 0x2902
READ = 0x02
WRITE = 0x08
NOTIFY = 0x10

# The phone's local time, 2031-05-06 07:08:09 on a Tuesday, an hour ahead of UTC.
PHONE_CURRENT_TIME = struct.pack("<HBBBBBBBB", 2031, 5, 6, 7, 8, 9, 2, 0, 0)
PHONE_ZONE_QUARTER_HOURS = 4
PHONE_LOCAL_TIME_INFORMATION = bytes([PHONE_ZONE_QUARTER_HOURS, 0])
PHONE_UTC_MILLISECONDS = calendar.timegm((2031, 5, 6, 6, 8, 9)) * 1000

# (handle, type, value) for each attribute, laid out as an Android phone might.
PHONE_TABLE = [
    (1, PRIMARY_SERVICE, struct.pack("<H", 0x1800)),
    (2, CHARACTERISTIC, struct.pack("<BHH", READ, 3, 0x2A00)),
    (3, 0x2A00, b"Phone"),
    (4, PRIMARY_SERVICE, struct.pack("<H", 0x1805)),
    (5, CHARACTERISTIC, struct.pack("<BHH", READ | NOTIFY, 6, 0x2A2B)),
    (6, 0x2A2B, PHONE_CURRENT_TIME),
    (7, CLIENT_CHARACTERISTIC_CONFIGURATION, b"\x00\x00"),
    (8, CHARACTERISTIC, struct.pack("<BHH", READ, 9, 0x2A0F)),
    (9, 0x2A0F, PHONE_LOCAL_TIME_INFORMATION),
    (10, PRIMARY_SERVICE, struct.pack("<H", 0x1811)),
    (11, CHARACTERISTIC, struct.pack("<BHH", READ, 12, 0x2A47)),
    (12, 0x2A47, b"\xff\x03"),
    (13, CHARACTERISTIC, struct.pack("<BHH", NOTIFY, 14, 0x2A46)),
    (14, 0x2A46, b""),
    (15, CLIENT_CHARACTERISTIC_CONFIGURATION, b"\x00\x00"),
    (16, CHARACTERISTIC, struct.pack("<BHH", WRITE, 17, 0x2A44)),
    (17, 0x2A44, b""),
]
NEW_ALERT_HANDLE = 14
NEW_ALERT_CONFIGURATION_HANDLE = 15
CONTROL_POINT_HANDLE = 17
NOTIFICATIONS_ON = b"\x01\x00"
ENABLE_EVERY_NEW_ALERT = b"\x02\xff"

# The default ATT MTU, which the watch never raises as a client, so each answer fits one bridge packet.
MTU = 23

ERROR_RESPONSE = 0x01
EXCHANGE_MTU_REQUEST = 0x02
EXCHANGE_MTU_RESPONSE = 0x03
FIND_INFORMATION_REQUEST = 0x04
FIND_INFORMATION_RESPONSE = 0x05
FIND_BY_TYPE_VALUE_REQUEST = 0x06
FIND_BY_TYPE_VALUE_RESPONSE = 0x07
READ_BY_TYPE_REQUEST = 0x08
READ_BY_TYPE_RESPONSE = 0x09
WRITE_REQUEST = 0x12
WRITE_RESPONSE = 0x13
NOTIFICATION = 0x1B
ATTRIBUTE_NOT_FOUND = 0x0A
REQUEST_NOT_SUPPORTED = 0x06


def group_end(service_handle):
    """The last handle of the service that starts here."""
    end = service_handle
    for handle, kind, _ in PHONE_TABLE:
        if handle > service_handle:
            if kind == PRIMARY_SERVICE:
                break
            end = handle
    return end


def answer(request, writes):
    """The phone's answer to one ATT request from the watch, as the standard lays it out."""
    opcode = request[0]
    not_found = bytes([ERROR_RESPONSE, opcode]) + request[1:3] + bytes([ATTRIBUTE_NOT_FOUND])
    if opcode == EXCHANGE_MTU_REQUEST:
        return struct.pack("<BH", EXCHANGE_MTU_RESPONSE, MTU)
    if opcode == WRITE_REQUEST:
        writes.append((struct.unpack_from("<H", request, 1)[0], request[3:]))
        return bytes([WRITE_RESPONSE])
    if opcode not in (FIND_BY_TYPE_VALUE_REQUEST, READ_BY_TYPE_REQUEST, FIND_INFORMATION_REQUEST):
        return bytes([ERROR_RESPONSE, opcode]) + request[1:3] + bytes([REQUEST_NOT_SUPPORTED])
    start, end = struct.unpack_from("<HH", request, 1)
    in_range = [attribute for attribute in PHONE_TABLE if start <= attribute[0] <= end]
    if opcode == FIND_BY_TYPE_VALUE_REQUEST:
        wanted = request[7:]
        found = [handle for handle, kind, value in in_range if kind == PRIMARY_SERVICE and value == wanted]
        if not found:
            return not_found
        return bytes([FIND_BY_TYPE_VALUE_RESPONSE]) + b"".join(
            struct.pack("<HH", handle, group_end(handle)) for handle in found[:(MTU - 1) // 4])
    if opcode == READ_BY_TYPE_REQUEST:
        wanted = struct.unpack_from("<H", request, 5)[0]
        found = [(handle, value[:MTU - 4]) for handle, kind, value in in_range if kind == wanted]
        if not found:
            return not_found
        size = len(found[0][1])
        entries = b""
        for handle, value in found:
            if len(value) != size or 2 + len(entries) + 2 + size > MTU:
                break
            entries += struct.pack("<H", handle) + value
        return bytes([READ_BY_TYPE_RESPONSE, size + 2]) + entries
    if not in_range:
        return not_found
    entries = b"".join(struct.pack("<HH", handle, kind) for handle, kind, _ in in_range[:(MTU - 2) // 4])
    return bytes([FIND_INFORMATION_RESPONSE, 1]) + entries


# ============================================================================
# Playing the phone
# ============================================================================

# How long the watch may take to find the services, in wall clock seconds; it starts 3 emulated seconds after connecting.
DISCOVERY_SECONDS = 40
KEPT_CLOCK_SYMBOL = "battletime_firmware::clock::KEPT_CLOCK"
KEPT_CLOCK_MARKER = 0x454D_4954
# The kept clock is written twice a second, and the phone's time is a few seconds old once it is read.
CLOCK_TOLERANCE_MILLISECONDS = 10_000
SMS_ALERT = bytes([5, 1]) + b"Lunch at 1?"
CALL_ALERT = bytes([3, 1]) + b"Sam"
BUZZ_SECONDS = 4


def serve_until(watch, writes, done, seconds):
    """Answers the watch's ATT requests until `done` holds or the time runs out; gives each request's opcode."""
    requests = []
    end = time.monotonic() + seconds
    while not done() and time.monotonic() < end:
        heard = False
        for kind, code, payload in watch.port.packets(packages_over_hci.NUDGE_AFTER_SECONDS):
            if kind == EVENT_PACKET and code == DISCONNECTION_COMPLETE:
                raise packages_over_hci.Disconnected(f"disconnected, reason 0x{payload[3]:02X}")
            if kind == ACL_PACKET and int.from_bytes(payload[2:4], "little") == ATTRIBUTE_CHANNEL:
                request = payload[4:]
                requests.append(request[0])
                watch.send(answer(request, writes))
                heard = True
                break
        if not heard:
            # The emulator's bridge controller sometimes holds a data packet until more HCI traffic comes.
            watch.port.write(packages_over_hci.command(packages_over_hci.READ_BD_ADDRESS))
    return requests


def notify(watch, handle, value):
    watch.send(struct.pack("<BH", NOTIFICATION, handle) + value)


def kept_utc_in_milliseconds(engine, address):
    """The UTC the firmware last kept for a reset, or None while it keeps no whole clock."""
    marker, utc_low, utc_high = struct.unpack("<3I", engine.read_bytes(address, 12))
    return (utc_high << 32 | utc_low) if marker == KEPT_CLOCK_MARKER else None


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    firmware = Path(sys.argv[1]).resolve()
    screenshot = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
    open_app_address, open_app_size = check_call.symbol(firmware, check_call.OPEN_APP_SYMBOL)
    kept_clock_address, _ = check_call.symbol(firmware, KEPT_CLOCK_SYMBOL)
    emulator = check_call.emulator_folder()
    results = []

    with tempfile.TemporaryDirectory(prefix="check-phone-services-") as temporary:
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
        script = folder / "phone_services.resc"
        script.write_text("\n".join(lines) + "\n")
        link = folder / "hci"

        engine = check_call.Engine(emulator, script, link)
        try:
            if not link.exists():
                sys.exit("the engine never opened its Bluetooth bridge")
            face = check_call.wait_for_open_app(engine, open_app_address, open_app_size,
                                                lambda current: current == check_call.FACE_ID)
            results.append(check_call.check("face open at start", face == check_call.FACE_ID, f"open app {face!r}"))

            port = packages_over_hci.HciPort(str(link))
            port.start_controller()
            watch = packages_over_hci.Watch.connect(port)
            writes = []
            requests = serve_until(watch, writes, lambda: any(handle == CONTROL_POINT_HANDLE for handle, _ in writes),
                                   DISCOVERY_SECONDS)
            results.append(check_call.check("the watch asks the phone by itself", bool(requests),
                                            "requests " + " ".join(f"{opcode:02x}" for opcode in requests)))
            wanted_writes = [(NEW_ALERT_CONFIGURATION_HANDLE, NOTIFICATIONS_ON), (CONTROL_POINT_HANDLE, ENABLE_EVERY_NEW_ALERT)]
            results.append(check_call.check("New Alert turned on and every category asked for", writes == wanted_writes,
                                            "writes " + ", ".join(f"{handle}={value.hex()}" for handle, value in writes)))

            time.sleep(1)
            kept = kept_utc_in_milliseconds(engine, kept_clock_address)
            off_by = None if kept is None else kept - PHONE_UTC_MILLISECONDS
            results.append(check_call.check("clock set from the phone's time and zone",
                                            off_by is not None and 0 <= off_by < CLOCK_TOLERANCE_MILLISECONDS,
                                            f"kept UTC {kept}, {off_by} ms after the phone's"))

            engine.sampler.start()
            notified_at = engine.emulated_seconds()
            notify(watch, NEW_ALERT_HANDLE, SMS_ALERT)
            serve_until(watch, writes, lambda: engine.emulated_seconds() > notified_at + BUZZ_SECONDS, BUZZ_SECONDS * 5)
            starts = check_call.motor_starts(engine.samples, notified_at, notified_at + BUZZ_SECONDS)
            results.append(check_call.check("an SMS alert buzzes", bool(starts),
                                            f"{len(starts)} motor starts in {BUZZ_SECONDS} s"))

            notify(watch, NEW_ALERT_HANDLE, CALL_ALERT)
            serve_until(watch, writes, lambda: check_call.open_app(engine, open_app_address, open_app_size)
                        == check_call.CALL_APP_ID, check_call.SECONDS_TO_SWITCH)
            current = check_call.open_app(engine, open_app_address, open_app_size)
            results.append(check_call.check("a call alert opens the call app", current == check_call.CALL_APP_ID,
                                            f"open app {current!r}"))
            if screenshot:
                engine.command(f"lcd SaveScreenshot @{screenshot}")
        finally:
            engine.stop()

    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
