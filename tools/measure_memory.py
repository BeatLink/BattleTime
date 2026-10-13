#!/usr/bin/env python3
"""Measures what fills the firmware slot, the RAM and the file system, for docs/memory-map.md.

Run it inside `nix develop` from anywhere in the repository:

    python3 -I tools/measure_memory.py

It builds the firmware twice, the full build and `--features recovery`, and sorts every symbol by
the crate that owns it. The firmware is linked with LTO into one object, so a linker map cannot
say where a byte came from; demangled symbol paths can. Unnamed constants are matched by content
against the files the firmware embeds (fonts, built-in apps, the accelerometer's configuration).
Then it compiles a typical install, the face, the launcher and the apps in TYPICAL_APPS, and runs
the `filesystem_use` example of neotime-packages, which installs them into a flash store mounted
as the watch mounts it.

It writes its tables into docs/memory-map.md, between the `<!-- measure_memory:NAME -->` markers.
With `--check` it changes nothing and fails when the doc's tables are out of date. CI's Memory map
job runs it on every push, and on master commits the doc when its tables change.
"""

import argparse
import pathlib
import re
import subprocess
import sys

REPOSITORY = pathlib.Path(__file__).resolve().parent.parent
FIRMWARE = REPOSITORY / "firmware"
MEMORY_MAP = REPOSITORY / "docs" / "memory-map.md"
FIRMWARE_START = 0x8100
FIRMWARE_SLOT_BYTES = 388 * 1024 - 256
RAM_START = 0x2000_0000
# The alive marker keeps the last 16 bytes of RAM.
RAM_FOR_THE_FIRMWARE = 64 * 1024 - 16

# ============================================================================
# Who owns a symbol
# ============================================================================

# Crate name prefixes, in the order they are tried, and the component each belongs to.
COMPONENTS = [
    ("neotime_app_host", "App host (with `neotime-ui`)"),
    ("neotime_ui", "App host (with `neotime-ui`)"),
    ("neotime_runtime", "App runtime"),
    ("neotime_bluetooth", "Bluetooth (controller and TrouBLE)"),
    ("trouble_host", "Bluetooth (controller and TrouBLE)"),
    ("bt_hci", "Bluetooth (controller and TrouBLE)"),
    ("RADIO", "Bluetooth (controller and TrouBLE)"),
    ("neotime_blefs", "BLEFS file service"),
    ("neotime_packages", "Packages"),
    ("neotime_settings", "Settings, companion and notifications (with serde)"),
    ("neotime_companion", "Settings, companion and notifications (with serde)"),
    ("neotime_notifications", "Settings, companion and notifications (with serde)"),
    ("serde", "Settings, companion and notifications (with serde)"),
    ("neotime_dfu", "Updates (DFU)"),
    ("neotime_updates", "Updates (DFU)"),
    ("embassy_boot", "Updates (DFU)"),
    ("neotime_flash_store", "Storage (flash store)"),
    ("neotime_storage", "Storage (littlefs, wasp-os rehearsal only)"),
    ("littlefs_rust", "Storage (littlefs, wasp-os rehearsal only)"),
    ("neotime_display", "Display, graphics and drivers"),
    ("neotime_touch", "Display, graphics and drivers"),
    ("neotime_accelerometer", "Display, graphics and drivers"),
    ("neotime_button", "Display, graphics and drivers"),
    ("neotime_vibrator", "Display, graphics and drivers"),
    ("neotime_external_flash", "Display, graphics and drivers"),
    ("mipidsi", "Display, graphics and drivers"),
    ("embedded_graphics", "Display, graphics and drivers"),
    ("embassy_nrf", "Display, graphics and drivers"),
    ("nrf_pac", "Display, graphics and drivers"),
    ("neotime_firmware", "Firmware (start-up, shell, tasks)"),
    ("neotime_boot", "Firmware (start-up, shell, tasks)"),
    ("embassy_", "Embassy, allocator and Cortex-M"),
    ("_embassy", "Embassy, allocator and Cortex-M"),
    ("GPIOTE", "Embassy, allocator and Cortex-M"),
    ("__INTERRUPTS", "Embassy, allocator and Cortex-M"),
    ("__RESET_VECTOR", "Embassy, allocator and Cortex-M"),
    ("__EXCEPTIONS", "Embassy, allocator and Cortex-M"),
    ("cortex_m", "Embassy, allocator and Cortex-M"),
    ("critical_section", "Embassy, allocator and Cortex-M"),
    ("embedded_alloc", "Embassy, allocator and Cortex-M"),
    ("linked_list_allocator", "Embassy, allocator and Cortex-M"),
    ("heapless", "Embassy, allocator and Cortex-M"),
    ("static_cell", "Embassy, allocator and Cortex-M"),
    ("core", "core, alloc and compiler builtins"),
    ("alloc", "core, alloc and compiler builtins"),
    ("compiler_builtins", "core, alloc and compiler builtins"),
    ("libm", "core, alloc and compiler builtins"),
    ("__aeabi", "core, alloc and compiler builtins"),
    ("mem", "core, alloc and compiler builtins"),
]

# Files the firmware embeds with include_bytes!, by the component they count towards.
EMBEDDED_FILES = [
    ("app-host/fonts/*.ntfont", "Fonts"),
]

UNATTRIBUTED = "Other constants and padding"
HEAP = "Heap"


def owning_crate(name):
    """The first path segment of the type or function a symbol belongs to.

    A task's poll function is named after embassy's TaskStorage, so the task's own crate is used.
    """
    task = re.search(r"TaskStorage<([a-z_0-9]+)::", name)
    if task:
        return task.group(1)
    return re.match(r"[<&\s]*(?:dyn\s+)?([A-Za-z_0-9]+)", name).group(1)


def component_of(crate):
    for prefix, component in COMPONENTS:
        if crate.startswith(prefix):
            return component
    return UNATTRIBUTED


# ============================================================================
# Reading a build
# ============================================================================


def run(command, directory):
    return subprocess.run(command, cwd=directory, check=True, capture_output=True, text=True).stdout


def build(features):
    """Builds the firmware and returns its symbols, its section sizes and its flash image."""
    flags = ["--features", features] if features else []
    run(["cargo", "build", "-q", "--release", *flags], FIRMWARE)
    symbols = run(["cargo", "nm", "-q", "--release", *flags, "--", "--print-size", "--demangle"], FIRMWARE)
    sections = run(["cargo", "size", "-q", "--release", *flags, "--", "-A"], FIRMWARE)
    image_path = REPOSITORY / "target" / f"measure-{features or 'full'}.bin"
    image_path.parent.mkdir(exist_ok=True)
    run(["cargo", "objcopy", "-q", "--release", *flags, "--", "-O", "binary", str(image_path)], FIRMWARE)
    section_sizes = {}
    for line in sections.splitlines():
        fields = line.split()
        if len(fields) == 3 and fields[0].startswith(".") and fields[1].isdigit():
            section_sizes[fields[0]] = int(fields[1])
    return symbols, section_sizes, image_path.read_bytes()


def embedded_files():
    """Each embedded file's bytes, with the component it counts towards."""
    found = []
    for pattern, component in EMBEDDED_FILES:
        for path in sorted(REPOSITORY.glob(pattern)):
            found.append((path.read_bytes(), component))
    return found


def breakdown(features):
    """Bytes per component in flash (code and constants) and in RAM (.data and .bss)."""
    symbols, sections, image = build(features)
    files = embedded_files()
    flash = {}
    ram = {}
    # Embedded files found by symbol; the rest are searched for in the image, since LTO often leaves them unnamed.
    matched = set()
    for line in symbols.splitlines():
        fields = line.split(maxsplit=3)
        if len(fields) != 4:
            continue
        address, size, kind, name = int(fields[0], 16), int(fields[1], 16), fields[2], fields[3]
        if size == 0:
            continue
        if kind in "bBdD":
            component = HEAP if name.endswith("HEAP_MEMORY") else component_of(owning_crate(name))
            ram[component] = ram.get(component, 0) + size
            if kind in "dD":
                # The initial values of .data are copied from flash too.
                flash[component] = flash.get(component, 0) + size
            continue
        if kind not in "tTrR":
            continue
        component = None
        if name.startswith(".L"):
            offset = address - FIRMWARE_START
            content = image[offset : offset + size]
            match = next((index for index, (data, _) in enumerate(files) if data == content), None)
            if match is not None:
                matched.add(match)
                component = files[match][1]
        component = component or component_of(owning_crate(name.lstrip(".")))
        flash[component] = flash.get(component, 0) + size

    for index, (data, component) in enumerate(files):
        if index not in matched and image.find(data) >= 0:
            flash[component] = flash.get(component, 0) + len(data)

    used = sections[".vector_table"] + sections[".text"] + sections[".rodata"] + sections[".data"]
    flash[UNATTRIBUTED] = flash.get(UNATTRIBUTED, 0) + used - sum(flash.values())
    static_ram = sections[".data"] + sections[".bss"] + sections.get(".uninit", 0)
    return {
        "flash": flash,
        "flash_used": used,
        "ram": ram,
        "static_ram": static_ram,
        "stack": RAM_FOR_THE_FIRMWARE - static_ram,
    }


# ============================================================================
# The file system of a typical install
# ============================================================================


# A typical install: one face, the launcher and the apps most people keep.
TYPICAL_FACE = "digital"
TYPICAL_APPS = ["alarm", "timer", "stopwatch", "settings", "weather", "steps", "heart", "flashlight", "music", "calculator"]


def compile_typical_install():
    """Compiles the typical install's images and returns them as ID=FILE pairs."""
    run(["cargo", "build", "-q", "-p", "neotime-compiler"], REPOSITORY)
    compiler = str(REPOSITORY / "target" / "debug" / "neotime-compiler")
    folder = REPOSITORY / "target" / "measure-apps"
    folder.mkdir(parents=True, exist_ok=True)
    sources = [(TYPICAL_FACE, REPOSITORY / "faces" / TYPICAL_FACE / "face.tsx")]
    sources += [(app, REPOSITORY / "apps" / app / "app.tsx") for app in ["launcher", *TYPICAL_APPS]]
    pairs = []
    for app_id, source in sources:
        image = folder / f"{app_id}.ntapp"
        run([compiler, str(source), "-o", str(image)], REPOSITORY)
        pairs.append(f"{app_id}={image}")
    return pairs


def file_system_use():
    pairs = compile_typical_install()
    output = run(
        ["cargo", "run", "-q", "-p", "neotime-packages", "--example", "filesystem_use", "--", *pairs],
        REPOSITORY,
    )
    rows = [line.split("\t") for line in output.splitlines()]
    values = {row[0]: int(row[1]) for row in rows if len(row) == 2}
    steps = [(row[0], int(row[1]), int(row[2])) for row in rows if len(row) == 3]
    return values, steps


# ============================================================================
# Writing the tables
# ============================================================================

# RAM owners are named for what they hold.
RAM_LABELS = {
    HEAP: "Heap (system 10 KB, UI 21 KB; the recovery build has no UI heap)",
    "Firmware (start-up, shell, tasks)": "Firmware: task storage, the Bluetooth host's resources and packet pool",
    "Bluetooth (controller and TrouBLE)": "Bluetooth controller",
    UNATTRIBUTED: "Other",
}


def kilobytes(size):
    return f"{size / 1024:.1f} KB"


def firmware_table(full, recovery):
    lines = ["| Component | Full build | Recovery build |", "|---|---:|---:|"]
    names = sorted(set(full["flash"]) | set(recovery["flash"]), key=lambda name: -full["flash"].get(name, 0))
    names.remove(UNATTRIBUTED)
    for name in names + [UNATTRIBUTED]:
        lines.append(f"| {name} | {kilobytes(full['flash'].get(name, 0))} | {kilobytes(recovery['flash'].get(name, 0))} |")
    lines.append(f"| **Used** | **{kilobytes(full['flash_used'])}** | **{kilobytes(recovery['flash_used'])}** |")
    free = [FIRMWARE_SLOT_BYTES - build["flash_used"] for build in (full, recovery)]
    lines.append(f"| Free of the 388 KB slot, after its header | {kilobytes(free[0])} | {kilobytes(free[1])} |")
    return lines


def ram_table(full, recovery):
    lines = ["| Owner | Full build | Recovery build |", "|---|---:|---:|"]
    names = sorted(set(full["ram"]) | set(recovery["ram"]), key=lambda name: -full["ram"].get(name, 0))
    for name in names:
        label = RAM_LABELS.get(name, name)
        lines.append(f"| {label} | {full['ram'].get(name, 0):,} B | {recovery['ram'].get(name, 0):,} B |")
    lines.append(f"| **Static RAM** (`.data` and `.bss`) | **{full['static_ram']:,} B** | **{recovery['static_ram']:,} B** |")
    lines.append(f"| Left for the stack | {full['stack']:,} B | {recovery['stack']:,} B |")
    return lines


def file_system_table(values, steps):
    sector_size = values["sector_size"]
    file_sectors = values["file_sectors"]
    record_ring_bytes = values["record_sectors"] * sector_size
    settings = [step for step in steps if step[0].startswith("/settings/")]
    images = [step for step in steps if step[0].endswith(".ntapp")]
    smallest = min(images, key=lambda step: step[1])
    largest = max(images, key=lambda step: step[1])

    def app_id(step):
        return step[0].split("/")[2]

    image_bytes = sum(step[1] for step in images)
    in_use = values["file_sectors_in_use"]
    record_bytes = values["record_bytes_in_use"]
    return [
        "| What | Bytes | File sectors |",
        "|---|---:|---:|",
        f"| `/settings/system` and `/settings/theme`, kept as records | {sum(step[1] for step in settings):,} | "
        f"{sum(step[2] for step in settings)} |",
        f"| {len(images)} app images, {smallest[1]:,} B ({app_id(smallest)}) to {largest[1]:,} B ({app_id(largest)}); "
        f"those of 1 KB or less are records | {image_bytes:,} | {sum(step[2] for step in images)} |",
        f"| **In use** | | **{in_use} of {file_sectors}: {in_use * sector_size // 1024} KB of "
        f"{file_sectors * sector_size // 1024 // 1024} MB, and {record_bytes:,} B of records in the "
        f"{record_ring_bytes // 1024} KB records ring** |",
    ]


def with_section(document, name, lines):
    """The document with the table between the named markers replaced."""
    start = f"<!-- measure_memory:{name} -->"
    end = f"<!-- /measure_memory:{name} -->"
    pattern = re.compile(re.escape(start) + r".*?" + re.escape(end), re.DOTALL)
    if not pattern.search(document):
        raise SystemExit(f"docs/memory-map.md has no {start} marker")
    return pattern.sub(lambda _: "\n".join([start, *lines, end]), document)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail if docs/memory-map.md is out of date, changing nothing")
    arguments = parser.parse_args()

    full = breakdown(None)
    recovery = breakdown("recovery")
    values, steps = file_system_use()
    document = MEMORY_MAP.read_text()
    updated = with_section(document, "firmware", firmware_table(full, recovery))
    updated = with_section(updated, "ram", ram_table(full, recovery))
    updated = with_section(updated, "file-system", file_system_table(values, steps))
    if updated == document:
        print("docs/memory-map.md is up to date")
        return 0
    if arguments.check:
        print("docs/memory-map.md is out of date; run python3 -I tools/measure_memory.py", file=sys.stderr)
        return 1
    MEMORY_MAP.write_text(updated)
    print("updated docs/memory-map.md")
    return 0


if __name__ == "__main__":
    sys.exit(main())
