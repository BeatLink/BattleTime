"""Checks the heart rate sensor end to end in the PineTime emulator: the firmware reads the pulse the emulated HRS3300 gives.

It runs three watches. On the first, with a face, the launcher and the heart app installed, it
opens the heart app from the launcher, sets the emulated pulse, and reads the firmware's
diagnostics counters for the sensor's state and its latest reading; it then changes the pulse and
checks the reading follows. The second has the heart rate interval setting on always and no app
open, so the reading must come from background measuring alone.

A third watch runs with the emulator's Bluetooth bridge and checks the Heart Rate service (0x180D)
through `tools/packages_over_hci.py`, without BlueZ: a phone subscribed to the measurement while
nothing measures hears nothing and starts nothing, as on InfiniTime; once the companion app turns
the interval setting to always, each reading is notified as the flags byte and the pulse, a read
gives the latest one, and turning notifications off stops them.

    python3 -I tools/check_heart_rate.py FIRMWARE_ELF [SCREENSHOT]

Run it inside `nix develop`, with the firmware built (`cd battletime/firmware && cargo build --release`). It
finds the emulator in $PINETIME_EMULATOR, or in a PineTimeEmulator folder beside the main checkout.
"""

import os
import re
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

# The other tools sit beside this script; importing them should leave no bytecode in the repository.
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_call  # noqa: E402
import packages_over_hci  # noqa: E402

REPOSITORY = Path(__file__).resolve().parent.parent
# The watch's crates, apps and build output.
BATTLETIME = REPOSITORY / "battletime"

# ============================================================================
# The install and the watch
# ============================================================================

PACKAGES = [("digital", "faces/digital/face.tsx"), ("launcher", "apps/launcher/app.tsx"), ("heart", "apps/heart/app.tsx")]
FIRMWARE_VECTOR_TABLE = 0x8100
IMAGE_HEADER_SIZE = 256
# The UICR words embassy-nrf writes on the first start, so the emulated chip is not reset.
FIRST_START_UICR = [(0x10001200, 0x15), (0x10001204, 0x15), (0x1000120C, 0xFFFFFFFE)]
COUNTERS_SYMBOL = "battletime_firmware::diagnostics::COUNTERS"
# The heart rate counters are the last two of diagnostics.rs: the sensor's state, then the latest reading.
STARTED = 1
FIRST_PULSE = 84
SECOND_PULSE = 120
BACKGROUND_PULSE = 66
# One window of samples takes about eight seconds, and later readings come every four.
FIRST_READING_SECONDS = 12
NEXT_READING_SECONDS = 10
LARGEST_ERROR = 2
# `/settings/system` with one record: the format byte, then setting 12 (the heart rate interval) at 1, always.
ALWAYS_MEASURING_SETTINGS = bytes([1, 12]) + struct.pack("<i", 1)

HEART_RATE_MEASUREMENT_UUID = packages_over_hci.uuid_bytes("2a37")
BLUETOOTH_PULSE = 78
BLUETOOTH_SECOND_PULSE = 110
# The companion app's line that turns the heart rate interval setting to always.
MEASURE_ALWAYS_LINE = '{"t":"settings","heart_rate_interval":1}'
# Long enough for a reading, had subscribing started the sensor.
SUBSCRIBED_IDLE_SECONDS = FIRST_READING_SECONDS + 3
# How long a wanted notification may take, in emulated seconds.
NOTIFICATION_SECONDS = 25
# How long the watch must stay quiet once notifications are turned off.
UNSUBSCRIBED_SECONDS = 10
# The flags byte InfiniTime sends: the rate is one byte, with no other fields.
MEASUREMENT_FLAGS = 0


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


def compile_install(folder):
    """Compiles the packages and the shared library; gives the packages as ID=FILE pairs and the library."""
    run(["cargo", "build", "-q", "-p", "battletime-compiler"])
    compiler = str(BATTLETIME / "target" / "debug" / "battletime-compiler")
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


def write_flash(path, entries):
    """Writes the external flash image holding these ID=FILE packages and /PATH=FILE files."""
    run(["cargo", "run", "-q", "-p", "battletime-packages", "--example", "filesystem_use", "--",
         "--no-theme", "--image", str(path), *entries])
    return path


# ============================================================================
# Running it
# ============================================================================

def run_watch(emulator, firmware, library, flash, steps, folder):
    """Starts the firmware on this install, runs the steps, and gives the emulator's output."""
    lines = [f"include @{emulator}/scripts/pinetime.resc"]
    lines += [f"sysbus WriteDoubleWord {address:#x} {value:#x}" for address, value in FIRST_START_UICR]
    lines += [
        f"sysbus LoadELF @{firmware}",
        *library_lines(library, folder),
        f"externalFlashMemory LoadBinary @{flash} 0",
        f"cpu VectorTableOffset {FIRMWARE_VECTOR_TABLE:#x}",
        f"cpu SP `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE:#x}`",
        f"cpu PC `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE + 4:#x}`",
    ]
    lines += steps + ["quit"]
    script = folder / "heart_rate.resc"
    script.write_text("\n".join(lines) + "\n")
    engine = subprocess.run(
        ["nix", "develop", str(emulator), "-c", "pinetime-engine", str(script)],
        capture_output=True, text=True, cwd=emulator, stdin=subprocess.DEVNULL, timeout=900,
    )
    return engine.stdout + engine.stderr


def readings(output, counters_size, expected):
    """The sensor's state and reading from each read of the counters, or exits with what the emulator said."""
    read = bytes(int(byte, 16) for byte in re.findall(r"0x([0-9A-Fa-f]{2})\b", output))
    if len(read) < counters_size * expected:
        sys.exit("the emulator gave too few readings:\n" + output[-4000:])
    words = [struct.unpack_from(f"<{counters_size // 4}I", read, index * counters_size) for index in range(expected)]
    return [(counters[-2], counters[-1]) for counters in words]


def check(name, pulse, reading):
    state, beats_per_minute = reading
    passed = state == STARTED and abs(beats_per_minute - pulse) <= LARGEST_ERROR
    print(f"{'ok  ' if passed else 'FAIL'}  {name}, pulse {pulse}: sensor state {state}, reading {beats_per_minute}")
    return passed


# ============================================================================
# The Heart Rate service over the Bluetooth bridge
# ============================================================================

def wait_emulated(engine, watch, seconds):
    """Lets this many emulated seconds pass, taking in the watch's notifications meanwhile."""
    end = engine.emulated_seconds() + seconds
    while engine.emulated_seconds() < end:
        watch.wait(set(), 0.2)


def measurements(watch, value_handle):
    return [value for handle, value in watch.notifications if handle == value_handle]


def wait_for_measurement(engine, watch, value_handle, pulse):
    """The first measurement notified near this pulse, or None once the time runs out."""
    end = engine.emulated_seconds() + NOTIFICATION_SECONDS
    while engine.emulated_seconds() < end:
        for value in measurements(watch, value_handle):
            if len(value) == 2 and abs(value[1] - pulse) <= LARGEST_ERROR:
                return value
        watch.wait(set(), packages_over_hci.NUDGE_AFTER_SECONDS)
        # The bridge's controller sometimes holds a packet until more HCI traffic comes.
        watch.port.write(packages_over_hci.command(packages_over_hci.READ_BD_ADDRESS))
    return None


def check_measurement(name, pulse, value):
    passed = value is not None and value[0] == MEASUREMENT_FLAGS and abs(value[1] - pulse) <= LARGEST_ERROR
    print(f"{'ok  ' if passed else 'FAIL'}  {name}, pulse {pulse}: {value.hex(' ') if value else 'nothing'}")
    return passed


def check_service(emulator, firmware, library, flash, counters, counters_size, folder):
    """Runs a watch with the Bluetooth bridge and checks the Heart Rate service as a phone sees it."""
    lines = [f"include @{emulator}/scripts/pinetime.resc"]
    lines += [f"sysbus WriteDoubleWord {address:#x} {value:#x}" for address, value in FIRST_START_UICR]
    lines += [
        f"sysbus LoadELF @{firmware}",
        *library_lines(library, folder),
        f"externalFlashMemory LoadBinary @{flash} 0",
        f"cpu VectorTableOffset {FIRMWARE_VECTOR_TABLE:#x}",
        f"cpu SP `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE:#x}`",
        f"cpu PC `sysbus ReadDoubleWord {FIRMWARE_VECTOR_TABLE + 4:#x}`",
        f"sysbus.twi1.heartRateSensor SetHeartRate {BLUETOOTH_PULSE}",
        "start",
    ]
    script = folder / "service.resc"
    script.write_text("\n".join(lines) + "\n")
    link = folder / "hci"
    results = []

    engine = check_call.Engine(emulator, script, link)
    try:
        if not link.exists():
            sys.exit("the engine never opened its Bluetooth bridge")
        port = packages_over_hci.HciPort(str(link))
        port.start_controller()
        watch = packages_over_hci.Watch.connect(port)
        measurement = watch.characteristic_handle(HEART_RATE_MEASUREMENT_UUID)
        receive = watch.characteristic_handle(packages_over_hci.NORDIC_UART_RECEIVE_UUID)

        watch.subscribe(measurement)
        wait_emulated(engine, watch, SUBSCRIBED_IDLE_SECONDS)
        state = struct.unpack_from(f"<{counters_size // 4}I", engine.read_bytes(counters, counters_size))[-2]
        quiet = not measurements(watch, measurement) and state != STARTED
        results.append(check_call.check("subscribing alone measures nothing", quiet,
                                        f"sensor state {state}, {len(measurements(watch, measurement))} notifications"))

        watch.write_line(receive, MEASURE_ALWAYS_LINE)
        value = wait_for_measurement(engine, watch, measurement, BLUETOOTH_PULSE)
        results.append(check_measurement("notified once measuring", BLUETOOTH_PULSE, value))

        engine.command(f"sysbus.twi1.heartRateSensor SetHeartRate {BLUETOOTH_SECOND_PULSE}")
        value = wait_for_measurement(engine, watch, measurement, BLUETOOTH_SECOND_PULSE)
        results.append(check_measurement("notified as the pulse changes", BLUETOOTH_SECOND_PULSE, value))

        results.append(check_measurement("read gives the latest", BLUETOOTH_SECOND_PULSE, watch.read(measurement)))

        watch.subscribe(measurement, is_on=False)
        watch.notifications.clear()
        wait_emulated(engine, watch, UNSUBSCRIBED_SECONDS)
        count = len(measurements(watch, measurement))
        results.append(check_call.check("unsubscribed hears nothing", count == 0, f"{count} notifications"))
    finally:
        engine.stop()
    return results


# ============================================================================
# All three watches
# ============================================================================

def main():
    firmware = Path(sys.argv[1]).resolve()
    screenshot = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
    counters, counters_size = symbol(firmware, COUNTERS_SYMBOL)
    read_counters = f"sysbus ReadBytes {counters:#x} {counters_size:#x}"
    emulator = emulator_folder()

    with tempfile.TemporaryDirectory() as temporary:
        folder = Path(temporary)
        pairs, library = compile_install(folder)

        app_steps = [
            f"sysbus.twi1.heartRateSensor SetHeartRate {FIRST_PULSE}",
            'emulation RunFor "00:00:05"',
            'sysbus.serial1.touch Swipe "up"',
            'emulation RunFor "00:00:03"',
            "sysbus.serial1.touch Tap 120 40",
            f'emulation RunFor "00:00:{FIRST_READING_SECONDS:02}"',
            read_counters,
            f"sysbus.twi1.heartRateSensor SetHeartRate {SECOND_PULSE}",
            f'emulation RunFor "00:00:{NEXT_READING_SECONDS:02}"',
            read_counters,
        ]
        if screenshot:
            app_steps.append(f"lcd SaveScreenshot @{screenshot}")
        flash = write_flash(folder / "app.bin", pairs)
        app_readings = readings(run_watch(emulator, firmware, library, flash, app_steps, folder), counters_size, 2)

        settings = folder / "settings"
        settings.write_bytes(ALWAYS_MEASURING_SETTINGS)
        background_steps = [
            f"sysbus.twi1.heartRateSensor SetHeartRate {BACKGROUND_PULSE}",
            f'emulation RunFor "00:00:{5 + FIRST_READING_SECONDS:02}"',
            read_counters,
        ]
        flash = write_flash(folder / "background.bin", [*pairs, f"/settings/system={settings}"])
        background_reading = readings(
            run_watch(emulator, firmware, library, flash, background_steps, folder), counters_size, 1
        )

        flash = write_flash(folder / "service.bin", pairs)
        service_results = check_service(emulator, firmware, library, flash, counters, counters_size, folder)

    results = [
        check("heart app", FIRST_PULSE, app_readings[0]),
        check("heart app", SECOND_PULSE, app_readings[1]),
        check("background, no app open", BACKGROUND_PULSE, background_reading[0]),
        *service_results,
    ]
    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
