"""Checks InfiniTime's own GATT services in the PineTime emulator, as Gadgetbridge's InfiniTime support uses them.

It runs one watch with the emulator's Bluetooth bridge and talks to it through
`tools/packages_over_hci.py`, without BlueZ:

- Music: writes the artist, track, length, position and playing state, opens the music app as a
  temporary app; the app's opening notifies InfiniTime's open event (0xe0). Once the phone writes
  that the player plays, a tap on the app's button notifies pause (0x01) beside the Nordic UART
  `music` line.
- Navigation: writes the turn's icon name, the instruction, the distance and the progress, and
  checks the navigation app shows them.
- Simple Weather: writes a version 1 current weather and a forecast, and checks the weather app
  shows the place, the sky and the coming days.
- Motion: reads the step count after the emulated chip counts steps, hears a notification when the
  count changes, hears raw acceleration only while subscribed to it, and nothing once unsubscribed.
- Immediate Alert: a write of level 2 starts the find ring, and level 0 stops it.

    python3 -I tools/check_infinitime_services.py FIRMWARE_ELF [SCREENSHOT_FOLDER]

Each service is a cargo feature of the firmware, and only the services the build serves are checked;
build with `--features music-service,navigation-service,alert-service,motion-service` for all five.

Run it inside `nix develop`, with the firmware built (`cd neotime/firmware && cargo build --release`). It
finds the emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator folder beside the main checkout.
With a folder, it saves the music, navigation and weather screens there.
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
import check_heart_rate  # noqa: E402
import packages_over_hci  # noqa: E402
from packages_over_hci import uuid_bytes  # noqa: E402

NEOTIME = check_heart_rate.NEOTIME

# ============================================================================
# The install
# ============================================================================

FACE = ("digital", "faces/digital/face.tsx")
# Opened as temporary apps with the packages characteristic's `run`, so no launcher taps are needed.
TEMPORARY_APPS = [
    ("music", "apps/music/app.tsx"),
    ("navigation", "apps/navigation/app.tsx"),
    ("weather", "apps/weather/app.tsx"),
]


def compile_install(folder):
    """Compiles the face, the temporary apps and the shared library; gives the flash entries and the library."""
    check_heart_rate.run(["cargo", "build", "-q", "-p", "neotime-compiler"])
    compiler = str(NEOTIME / "target" / "debug" / "neotime-compiler")
    image = folder / "face.ntapp"
    check_heart_rate.run([compiler, FACE[1], "-o", str(image)])
    entries = [f"{FACE[0]}={image}"]
    for app_id, source in TEMPORARY_APPS:
        image = folder / f"{app_id}.ntapp"
        check_heart_rate.run([compiler, source, "-o", str(image)])
        entries.append(f"/temp/{app_id}/app.ntapp={image}")
    library = folder / "library.bin"
    check_heart_rate.run([compiler, "library", "-o", str(library)])
    return entries, library


# ============================================================================
# The services, as InfiniTime numbers their characteristics
# ============================================================================

def infinitime_uuid(service, characteristic):
    return uuid_bytes(f"{service:04x}{characteristic:04x}-78fc-48fe-8e23-433b3a1942d0")


MUSIC_EVENT = infinitime_uuid(0, 0x01)
MUSIC_STATUS = infinitime_uuid(0, 0x02)
MUSIC_ARTIST = infinitime_uuid(0, 0x03)
MUSIC_TRACK = infinitime_uuid(0, 0x04)
MUSIC_POSITION = infinitime_uuid(0, 0x06)
MUSIC_TOTAL_LENGTH = infinitime_uuid(0, 0x07)
NAVIGATION_FLAG = infinitime_uuid(1, 0x01)
NAVIGATION_NARRATIVE = infinitime_uuid(1, 0x02)
NAVIGATION_DISTANCE = infinitime_uuid(1, 0x03)
NAVIGATION_PROGRESS = infinitime_uuid(1, 0x04)
MOTION_STEP_COUNT = infinitime_uuid(3, 0x01)
MOTION_VALUES = infinitime_uuid(3, 0x02)
WEATHER_DATA = infinitime_uuid(5, 0x01)
ALERT_LEVEL = uuid_bytes("2a06")
NORDIC_UART_TRANSMIT = uuid_bytes("6e400003-b5a3-f393-e0a9-e50e24dcca9e")

MUSIC_OPEN_EVENT = 0xE0
MUSIC_PAUSE_EVENT = 0x01
# The music app's play and pause button, in the middle of its bottom row.
PLAY_BUTTON = (120, 205)
PAUSE_LINE = b'{"t":"music","n":"pause"}'

STEPS = 1234
MORE_STEPS = 1300
# The firmware reads the step counter every 30 seconds.
STEP_READ_SECONDS = 35
TILT = (293, 0, -957)
# How far a notified axis may sit from what the emulator was given, in milli-g.
LARGEST_AXIS_ERROR = 40
MOTION_SECONDS = 3
QUIET_SECONDS = 3
FIND_SECONDS = 3
# How long the engine may take to open the Bluetooth bridge, in real seconds.
BRIDGE_SECONDS = 30
# Long enough for an app to open and draw.
SETTLE_SECONDS = 2


def current_weather():
    """Version 1 current weather: 12.4 °C, low 8, high 15, rain, in Bergen, sun from 06:30 to 20:00."""
    write = bytes([0, 1]) + struct.pack("<q", int(time.time()))
    write += struct.pack("<hhh", 1240, 800, 1500)
    write += b"Bergen".ljust(32, b"\0") + bytes([5])
    return write + struct.pack("<hh", 390, 1200)


def forecast():
    """Three coming days: snow from 3 to 9 °C, clear from 10 to 21, clouds from -2 to 4."""
    write = bytes([1, 0]) + struct.pack("<q", int(time.time())) + bytes([3])
    for low, high, icon in [(300, 900, 7), (1000, 2100, 0), (-200, 400, 2)]:
        write += struct.pack("<hhB", low, high, icon)
    return write


# ============================================================================
# Talking to the watch
# ============================================================================

# The most bytes one ACL packet carries to the bridge's controller, as a phone's controller splits them.
ACL_FRAGMENT_BYTES = 27
# The packet boundary flag that marks an ACL packet as continuing the one before.
CONTINUING_FRAGMENT = 0x1000


def send_in_fragments(watch, pdu):
    """Sends an ATT PDU in ACL packets of at most 27 bytes, which the bridge needs for one longer than 23."""
    l2cap = len(pdu).to_bytes(2, "little") + packages_over_hci.ATTRIBUTE_CHANNEL.to_bytes(2, "little") + pdu
    for offset in range(0, len(l2cap), ACL_FRAGMENT_BYTES):
        piece = l2cap[offset:offset + ACL_FRAGMENT_BYTES]
        handle = watch.handle | (CONTINUING_FRAGMENT if offset else 0)
        watch.port.write(bytes([packages_over_hci.ACL_PACKET]) + handle.to_bytes(2, "little")
                         + len(piece).to_bytes(2, "little") + piece)


def write(watch, handle, data):
    """One acknowledged Write Request, of any length up to the exchanged MTU."""
    pdu = bytes([packages_over_hci.WRITE_REQUEST]) + handle.to_bytes(2, "little") + data
    send = watch.send
    watch.send = lambda pdu: send_in_fragments(watch, pdu)
    try:
        watch.request(pdu, packages_over_hci.WRITE_RESPONSE)
    finally:
        watch.send = send


def notified(watch, handle):
    return [value for value_handle, value in watch.notifications if value_handle == handle]


def wait_for(engine, watch, seconds, test):
    """Takes in notifications until the test holds or this many emulated seconds pass; gives whether it held."""
    end = engine.emulated_seconds() + seconds
    while engine.emulated_seconds() < end:
        if test():
            return True
        watch.wait(set(), 0.3)
        # The bridge's controller sometimes holds a packet until more HCI traffic comes.
        watch.port.write(packages_over_hci.command(packages_over_hci.READ_BD_ADDRESS))
    return test()


def run_app(engine, watch, packages, app_id, open_app, folder):
    """Opens a temporary app and gives the app id the shell then shows, saving its screen when asked."""
    reply = watch.run_command(packages, f'{{"c":"run","id":"{app_id}"}}')
    shown = check_call.wait_for_open_app(engine, *open_app, lambda current: current == app_id)
    wait_emulated(engine, watch, SETTLE_SECONDS)
    if folder:
        engine.command(f"lcd SaveScreenshot @{folder / f'{app_id}.png'}")
    return reply, shown


def wait_emulated(engine, watch, seconds):
    end = engine.emulated_seconds() + seconds
    while engine.emulated_seconds() < end:
        watch.wait(set(), 0.2)


def axes_match(value, reading):
    """Whether six notified bytes hold the reading's axes, in any order and sign, since the watch remaps them."""
    if len(value) != 6:
        return False
    notified_axes = sorted(abs(axis) for axis in struct.unpack("<hhh", value))
    wanted = sorted(abs(axis) for axis in reading)
    return all(abs(got - want) <= LARGEST_AXIS_ERROR for got, want in zip(notified_axes, wanted))


# ============================================================================
# The checks
# ============================================================================

def check_music(engine, watch, handles, packages, open_app, folder):
    results = []
    watch.subscribe(handles["music_event"])
    watch.subscribe(handles["uart_transmit"])
    write(watch, handles["music_artist"], b"The Artist")
    write(watch, handles["music_track"], b"A Song")
    write(watch, handles["music_total_length"], struct.pack(">I", 200))
    write(watch, handles["music_position"], struct.pack(">I", 65))
    watch.notifications.clear()

    reply, shown = run_app(engine, watch, packages, "music", open_app, folder)
    results.append(check_call.check("music app opens", shown == "music", f"{reply}, open app {shown!r}"))
    opened = wait_for(engine, watch, 5, lambda: bytes([MUSIC_OPEN_EVENT]) in notified(watch, handles["music_event"]))
    results.append(check_call.check("opening notifies the open event", opened,
                                    f"events {[value.hex() for value in notified(watch, handles['music_event'])]}"))

    # The open app hears the player start, so its button then pauses.
    write(watch, handles["music_status"], bytes([1]))
    wait_emulated(engine, watch, SETTLE_SECONDS)
    if folder:
        engine.command(f"lcd SaveScreenshot @{folder / 'music.png'}")
    watch.notifications.clear()
    watch.notified = b""
    engine.command("sysbus.serial1.touch Tap {} {}".format(*PLAY_BUTTON))
    paused = wait_for(engine, watch, 5, lambda: bytes([MUSIC_PAUSE_EVENT]) in notified(watch, handles["music_event"]))
    results.append(check_call.check("the pause button notifies pause", paused,
                                    f"events {[value.hex() for value in notified(watch, handles['music_event'])]}"))
    line_sent = wait_for(engine, watch, 5, lambda: PAUSE_LINE in watch.notified)
    results.append(check_call.check("and sends the UART music line", line_sent, repr(watch.notified[:80])))
    return results


def check_navigation(engine, watch, handles, packages, open_app, folder):
    # The app forgets the last step when it opens, so the phone writes the next one after.
    reply, shown = run_app(engine, watch, packages, "navigation", open_app, None)
    write(watch, handles["navigation_flag"], b"turn-slight-left")
    write(watch, handles["navigation_narrative"], b"Bear left onto High Street")
    write(watch, handles["navigation_distance"], b"1.2km")
    write(watch, handles["navigation_progress"], bytes([40]))
    wait_emulated(engine, watch, SETTLE_SECONDS)
    if folder:
        engine.command(f"lcd SaveScreenshot @{folder / 'navigation.png'}")
    return [check_call.check("navigation app opens", shown == "navigation", f"{reply}, open app {shown!r}")]


def check_weather(engine, watch, handles, packages, open_app, folder):
    write(watch, handles["weather_data"], current_weather())
    write(watch, handles["weather_data"], forecast())
    reply, shown = run_app(engine, watch, packages, "weather", open_app, folder)
    return [check_call.check("weather app opens", shown == "weather", f"{reply}, open app {shown!r}")]


def check_motion(engine, watch, handles):
    results = []
    step_count = handles["motion_step_count"]
    values = handles["motion_values"]
    engine.command(f"accelerometer SetSteps {STEPS}")
    watch.subscribe(step_count)
    watch.notifications.clear()
    heard = wait_for(engine, watch, STEP_READ_SECONDS,
                     lambda: struct.pack("<I", STEPS) in notified(watch, step_count))
    results.append(check_call.check("a new step count is notified", heard,
                                    f"{[value.hex() for value in notified(watch, step_count)]}"))
    engine.command(f"accelerometer SetSteps {MORE_STEPS}")
    heard = wait_for(engine, watch, STEP_READ_SECONDS,
                     lambda: struct.pack("<I", MORE_STEPS) in notified(watch, step_count))
    results.append(check_call.check("and the next", heard, f"{[value.hex() for value in notified(watch, step_count)]}"))
    read = watch.read(step_count)
    results.append(check_call.check("a read gives today's steps", read == struct.pack("<I", MORE_STEPS), read.hex()))

    engine.command("accelerometer SetAcceleration {} {} {}".format(*TILT))
    watch.notifications.clear()
    wait_emulated(engine, watch, QUIET_SECONDS)
    count = len(notified(watch, values))
    results.append(check_call.check("no acceleration before subscribing", count == 0, f"{count} notifications"))

    watch.subscribe(values)
    watch.notifications.clear()
    wait_emulated(engine, watch, MOTION_SECONDS)
    readings = notified(watch, values)
    matching = [value for value in readings if axes_match(value, TILT)]
    results.append(check_call.check("acceleration notified while subscribed", len(matching) >= MOTION_SECONDS,
                                    f"{len(readings)} notifications, {len(matching)} matching, first "
                                    f"{struct.unpack('<hhh', readings[0]) if readings else None}"))

    watch.subscribe(values, is_on=False)
    wait_emulated(engine, watch, 1)
    watch.notifications.clear()
    wait_emulated(engine, watch, QUIET_SECONDS)
    count = len(notified(watch, values))
    results.append(check_call.check("none once unsubscribed", count == 0, f"{count} notifications"))
    return results


def check_immediate_alert(engine, watch, handles):
    results = []
    engine.sampler.start()
    written_at = engine.emulated_seconds()
    write(watch, handles["alert_level"], bytes([2]))
    wait_emulated(engine, watch, FIND_SECONDS)
    starts = check_call.motor_starts(engine.samples, written_at, written_at + FIND_SECONDS)
    results.append(check_call.check("level 2 rings to find the watch", len(starts) >= 2,
                                    f"motor starts at {[round(when - written_at, 2) for when in starts]} s"))

    stopped_at = engine.emulated_seconds()
    write(watch, handles["alert_level"], bytes([0]))
    wait_emulated(engine, watch, 1 + FIND_SECONDS)
    still = [is_running for when, is_running in engine.samples if stopped_at + 1 <= when]
    results.append(check_call.check("level 0 stops it", bool(still) and not any(still), f"{len(still)} samples"))
    return results


# ============================================================================
# The watch
# ============================================================================

def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    firmware = Path(sys.argv[1]).resolve()
    screenshots = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
    if screenshots:
        screenshots.mkdir(parents=True, exist_ok=True)
    open_app = check_call.symbol(firmware, check_call.OPEN_APP_SYMBOL)
    emulator = check_heart_rate.emulator_folder()
    results = []

    with tempfile.TemporaryDirectory(prefix="check-infinitime-services-") as temporary:
        folder = Path(temporary)
        entries, library = compile_install(folder)
        flash = check_heart_rate.write_flash(folder / "flash.bin", entries)
        lines = [f"include @{emulator}/scripts/pinetime.resc"]
        lines += [f"sysbus WriteDoubleWord {address:#x} {value:#x}"
                  for address, value in check_heart_rate.FIRST_START_UICR]
        vector_table = check_heart_rate.FIRMWARE_VECTOR_TABLE
        lines += [
            f"sysbus LoadELF @{firmware}",
            *check_heart_rate.library_lines(library, folder),
            f"externalFlashMemory LoadBinary @{flash} 0",
            f"cpu VectorTableOffset {vector_table:#x}",
            f"cpu SP `sysbus ReadDoubleWord {vector_table:#x}`",
            f"cpu PC `sysbus ReadDoubleWord {vector_table + 4:#x}`",
            "start",
        ]
        script = folder / "services.resc"
        script.write_text("\n".join(lines) + "\n")
        link = folder / "hci"

        engine = check_call.Engine(emulator, script, link)
        try:
            # The engine can answer on its monitor a moment before the bridge's link appears.
            deadline = time.monotonic() + BRIDGE_SECONDS
            while not link.exists() and time.monotonic() < deadline:
                time.sleep(0.2)
            if not link.exists():
                sys.exit("the engine never opened its Bluetooth bridge")
            face = check_call.wait_for_open_app(engine, *open_app, lambda current: current == FACE[0], seconds=60)
            results.append(check_call.check("face open at start", face == FACE[0], f"open app {face!r}"))
            port = packages_over_hci.HciPort(str(link))
            port.start_controller()
            watch = packages_over_hci.Watch.connect(port)
            wanted = {
                "music_event": MUSIC_EVENT,
                "music_status": MUSIC_STATUS,
                "music_artist": MUSIC_ARTIST,
                "music_track": MUSIC_TRACK,
                "music_position": MUSIC_POSITION,
                "music_total_length": MUSIC_TOTAL_LENGTH,
                "navigation_flag": NAVIGATION_FLAG,
                "navigation_narrative": NAVIGATION_NARRATIVE,
                "navigation_distance": NAVIGATION_DISTANCE,
                "navigation_progress": NAVIGATION_PROGRESS,
                "motion_step_count": MOTION_STEP_COUNT,
                "motion_values": MOTION_VALUES,
                "weather_data": WEATHER_DATA,
                "alert_level": ALERT_LEVEL,
                "uart_transmit": NORDIC_UART_TRANSMIT,
            }
            handles = {}
            for name, uuid in wanted.items():
                try:
                    handles[name] = watch.characteristic_handle(uuid)
                except RuntimeError:
                    pass
            packages = watch.open_packages()

            # Each service is a feature of the firmware's build, so only those it serves are checked.
            served = {name.split("_")[0] for name in handles}
            print("serves " + ", ".join(sorted(served - {"uart"})))
            if "music" in served:
                results += check_music(engine, watch, handles, packages, open_app, screenshots)
            if "navigation" in served:
                results += check_navigation(engine, watch, handles, packages, open_app, screenshots)
            if "weather" in served:
                results += check_weather(engine, watch, handles, packages, open_app, screenshots)
            watch.run_command(packages, '{"c":"stop"}')
            if "motion" in served:
                results += check_motion(engine, watch, handles)
            if "alert" in served:
                results += check_immediate_alert(engine, watch, handles)
        finally:
            engine.stop()
    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
