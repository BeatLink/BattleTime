# Memory map

The PineTime has three kinds of memory: 512 KB of internal flash, 4 MB of external SPI flash and 64
KB of RAM. This page shows how NeoTime divides each one. Each box is drawn in proportion to its
size.

The flash layouts are set in code by the `neotime-boot` crate (`boot/src/lib.rs`), which the
bootloader and the firmware share, so that is the source of truth. [bootloader.md](bootloader.md)
explains how the bootloader uses them. The RAM layout is a planned budget, and each figure gets
replaced by a measurement as its part is built.

To redraw the pictures after a change, edit the sizes in `tools/draw_memory_maps.py` and run:

```sh
python3 tools/draw_memory_maps.py
```

## Internal flash (512 KB)

![Internal flash treemap](images/internal-flash.svg)

| Region | Start | Size | Holds |
|---|---|---|---|
| Bootloader | `0x00000` | 32 KB | The `embassy-boot` bootloader. There is no SoftDevice and no MBR. |
| Boot state | `0x08000` | 4 KB | The bootloader's record of an update in progress, for swapping and rolling back. |
| Firmware | `0x09000` | 384 KB | The running firmware. It is linked to start here. |
| Shared library | `0x69000` | 92 KB | What apps share, up to the end of flash: images, fonts, components, common functions and the preset themes. No apps are kept here; they live in `/apps` on the external flash. See [app-runtime.md](app-runtime.md#the-shared-library). |

Inside the firmware, 48 KB is set aside for the app runtime: the interpreter, the image verifier,
the scheduler and the code that turns an app's widget tree into rlvgl widgets. It is part of the
firmware image, so it runs in place from the internal flash, with the chip's instruction cache
turned on, and never needs copying into RAM. The figure is a planned budget, to be replaced by
the measured size once the runtime is built. See [app-runtime.md](app-runtime.md#the-runtime).

Also inside the firmware, the accelerometer's configuration files from Bosch take 6 KB for each chip
built in, 12 KB with both the BMA421 and the BMA425 (see the `neotime-accelerometer` cargo
features).

## External SPI flash (4 MB)

![External flash treemap](images/external-flash.svg)

The chip is an XT25F32B, on the same SPI bus as the screen.

| Region | Start | Size | Holds |
|---|---|---|---|
| Firmware update | `0x000000` | 388 KB | A new firmware received over Bluetooth, waiting for the bootloader to swap it in: the firmware's 384 KB plus one page. |
| Recovery firmware | `0x061000` | 384 KB | A known-good firmware to fall back to. |
| Reserved for growth | `0x0C1000` | 252 KB | Kept free so the slots above can grow, up to `0x100000`. |
| File system | `0x100000` | 3 MB | A littlefs file system, with the folders `/apps`, `/settings`, `/notifications` and `/resources`. |

Anything that changes often, such as settings, notifications and app state, is kept here, because
littlefs spreads its writes across the chip. The internal flash changes only when the firmware or
the shared library is updated.

## RAM (64 KB, planned)

![RAM treemap](images/ram.svg)

While the bootloader runs it uses all 64 KB from `0x20000000`. Nothing is kept when it jumps to the
firmware, which then has the whole of RAM:

| Region | Budget | Holds | Status |
|---|---|---|---|
| UI heap | 14 KB | The heap that `rlvgl`'s widgets allocate from. The `neotime-ui` crate keeps no screen buffer of its own. `neotime-app-host` keeps one 276 B node per widget of the open screen here, plus its text, so about 50 widgets fit. The stopwatch demo's screen is about 1.1 KB on the watch; on the computer it holds 1,327 B and peaks at 1,679 B while drawing. | Partly measured: node size with `size_of` for the watch, the demo screen with a counting allocator on the computer |
| Bluetooth | 12 KB | The controller and the TrouBLE host: connection state, packet buffers and the GATT tables. | Partly measured: the controller is 1.5 KB; TrouBLE's example with it is 12.0 KB in all, executor included |
| Apps | 12 KB | Memory for the app that is open, read from its file in `/apps`. The runtime's working memory in it is measured: 1,792 B for the interpreter, 1,032 B for the bindings and 2,440 B for the state. `AppHost`, which holds all three, is 5,544 B. | Partly measured |
| Stack | 8 KB | The main stack, for interrupts and code that is not in a task. | Planned |
| Tasks | 6 KB | Embassy's tasks. Each task's state is fixed in size when the firmware is built. | Planned |
| Reserve | 4 KB | Left free, so a budget that runs over does not crash the watch. | Planned |
| System state | 4 KB | Settings, notifications, alarms and the step count. | Planned |
| Display buffer | 2 KB | The buffer `neotime-display` fills before each SPI transfer. | Planned |
| File system buffers | 2 KB | littlefs: 760 B while mounted, plus 352 B for each open file; room for three open at once. | Measured |

The 64 KB is all the chip has, so a part that needs more must take it from another row of this
table.

## Watchdog

The bootloader leaves the watchdog running when it starts the firmware, with a 5 second timeout.
It keeps counting while the chip sleeps and pauses while a debugger has it halted. The firmware
must take it over with exactly those settings, because `embassy-nrf` refuses different ones.

The firmware feeds it only while the button is up, so holding the button for about 5 seconds
resets the watch into the bootloader, and holding it 5 seconds more installs the recovery firmware.
If the firmware hangs, the watch resets, and an update that never confirms itself is rolled back.
See [bootloader.md](bootloader.md) for the details.
