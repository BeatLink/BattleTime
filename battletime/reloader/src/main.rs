//! Moves a PineTime from InfiniTime or wasp-os onto BattleTime.
//!
//! The old system starts this image as an ordinary app. It stages the recovery firmware and the new
//! firmware in the external flash, writes BattleTime's bootloader over the start of internal flash, marks
//! the staged firmware for the bootloader to install, and restarts. The order comes from
//! `battletime_boot::migration`; see docs/updates.md for why a power cut only matters during the
//! bootloader write, which takes well under a second.

#![no_std]
#![no_main]

use core::cell::RefCell;

use battletime_battery::{battery_line, can_write_bootloader, Measurement, BATTERY_LINE_LENGTH};
use battletime_boot::{
    migration::{crc32, plan, PayloadSizes, Step, CRC32_INITIAL},
    BOOT_STATE, DEFAULT_SLOTS, PAGE_SIZE,
};
use battletime_display::{Display, MipidsiInterface};
use battletime_external_flash::{BusyWait, ExternalFlash};
use cortex_m::delay::Delay;
use cortex_m_rt::entry;
use embassy_boot::{AlignedBuffer, BlockingFirmwareUpdater, FirmwareUpdaterConfig};
use embassy_embedded_hal::{flash::partition::BlockingPartition, shared_bus::blocking::spi::SpiDevice};
use embassy_nrf::{
    bind_interrupts,
    gpio::{Input, Level, Output, OutputDrive, Pull},
    nvmc::Nvmc,
    peripherals,
    spim::{self, Spim},
};
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, Mutex};
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use embedded_storage::nor_flash::{ErrorType, NorFlash, ReadNorFlash};
use mipidsi::interface::SpiInterface;
// A panic halts, so the watchdog resets into the old update mode instead of looping through this image.
use panic_halt as _;
use static_cell::StaticCell;

mod payloads {
    pub struct Payload {
        pub bytes: &'static [u8],
        pub length_in_bytes: usize,
        pub crc32: u32,
    }

    impl Payload {
        pub fn data(&self) -> &'static [u8] {
            &self.bytes[..self.length_in_bytes]
        }

        /// Whole words, padded with 0xFF, which the internal flash wants.
        pub fn padded(&self) -> &'static [u8] {
            self.bytes
        }
    }

    include!(concat!(env!("OUT_DIR"), "/payloads.rs"));
}

use payloads::{Payload, BOOTLOADER, FIRMWARE, RECOVERY};

// ============================================================================
// PineTime wiring and timing
// ============================================================================

const CPU_FREQUENCY_IN_HERTZ: u32 = 64_000_000;
const WATCHDOG_RELOAD: u32 = 0x6E52_4635;
const WATCHDOG_RELOAD_REGISTER_0: *mut u32 = 0x4001_0600 as *mut u32;
const COPY_CHUNK_SIZE: usize = 1024;
const WRITE_RETRIES: u32 = 3;
/// 256-byte aligned RAM below everything the reloader uses, for its copy of the vector table.
const VECTOR_TABLE_IN_RAM: *mut u32 = 0x2000_0800 as *mut u32;
const VECTOR_COUNT: usize = 64;

type SpiBus = Mutex<NoopRawMutex, RefCell<Spim<'static>>>;

bind_interrupts!(struct Interrupts {
    TWISPI0 => spim::InterruptHandler<peripherals::TWISPI0>;
});

// ============================================================================
// The two words the old system starts us from
// ============================================================================

extern "C" {
    fn Reset() -> !;
    static _stack_start: u32;
}

#[repr(C)]
struct EntryStub {
    stack_pointer: &'static u32,
    reset_handler: unsafe extern "C" fn() -> !,
}

#[link_section = ".entry_stub"]
#[used]
static ENTRY_STUB: EntryStub = EntryStub {
    stack_pointer: unsafe { &_stack_start },
    reset_handler: Reset,
};

// ============================================================================
// Start
// ============================================================================

#[entry]
fn main() -> ! {
    own_the_vector_table();
    let chip = embassy_nrf::init(Default::default());
    let core = cortex_m::Peripherals::take().unwrap();
    let mut delay = Delay::new(core.SYST, CPU_FREQUENCY_IN_HERTZ);
    // The button reads high while held, and only while its power pin is driven high.
    let _button_power = Output::new(chip.P0_15, Level::High, OutputDrive::Standard);
    let button = Input::new(chip.P0_13, Pull::Down);

    static SPI_BUS: StaticCell<SpiBus> = StaticCell::new();
    let mut spi_config = spim::Config::default();
    spi_config.frequency = spim::Frequency::M8;
    let spi_bus = SPI_BUS.init(Mutex::new(RefCell::new(Spim::new(
        chip.TWISPI0,
        Interrupts,
        chip.P0_02,
        chip.P0_04,
        chip.P0_03,
        spi_config,
    ))));
    let Some(mut screen) = Screen::start(
        spi_bus,
        Output::new(chip.P0_25, Level::High, OutputDrive::Standard),
        Output::new(chip.P0_18, Level::Low, OutputDrive::Standard),
        Output::new(chip.P0_26, Level::High, OutputDrive::Standard),
        &mut delay,
    ) else {
        halt(&button);
    };
    let _backlight_on = Output::new(chip.P0_14, Level::Low, OutputDrive::Standard);
    screen.title("BattleTime installer");

    let battery = Battery {
        power_present: Input::new(chip.P0_19, Pull::Up),
        charging: Input::new(chip.P0_12, Pull::Up),
    };
    wait_until_the_battery_is_safe(&battery, &button, &mut screen, &mut delay);

    screen.status("Checking");
    if !payloads_are_intact() {
        screen.status("Damaged download");
        halt(&button);
    }

    let flash_chip_select = Output::new(chip.P0_05, Level::High, OutputDrive::Standard);
    let Ok(external_flash) = ExternalFlash::new(
        SpiDevice::new(spi_bus, flash_chip_select),
        BusyWait {
            cpu_frequency_in_hertz: CPU_FREQUENCY_IN_HERTZ,
        },
    ) else {
        screen.status("No flash chip");
        halt(&button);
    };
    let external_flash = Mutex::<NoopRawMutex, _>::new(RefCell::new(FeedsWatchdog {
        flash: external_flash,
    }));
    let internal_flash = Mutex::<NoopRawMutex, _>::new(RefCell::new(FeedsWatchdog {
        flash: Nvmc::new(chip.NVMC),
    }));

    let sizes = PayloadSizes {
        bootloader_in_bytes: BOOTLOADER.length_in_bytes as u32,
        firmware_in_bytes: FIRMWARE.length_in_bytes as u32,
        recovery_in_bytes: RECOVERY.as_ref().map(|recovery| recovery.length_in_bytes as u32),
    };
    let (steps, count) = plan(sizes);
    for step in &steps[..count] {
        let done = match *step {
            Step::StageRecovery { region } => {
                screen.status("Copying recovery");
                let recovery = RECOVERY.as_ref().unwrap();
                let mut slot = BlockingPartition::new(&external_flash, region.start, region.size);
                copy_with_retries(&mut slot, recovery, &mut screen)
            }
            Step::StageFirmware { .. } => {
                screen.status("Copying firmware");
                stage_firmware(&external_flash, &internal_flash, &mut screen)
            }
            Step::WriteBootloader { region } => {
                // Staging takes minutes, in which the charger may have come out.
                wait_until_the_battery_is_safe(&battery, &button, &mut screen, &mut delay);
                screen.status("Installing, do not unplug");
                let mut pages = BlockingPartition::new(&internal_flash, region.start, region.size);
                copy_with_retries(&mut pages, &BOOTLOADER, &mut screen)
            }
            Step::MarkSwap { .. } => {
                screen.status("Finishing");
                mark_swap(&external_flash, &internal_flash)
            }
        };
        if !done {
            screen.status("Failed, hold the button");
            halt(&button);
        }
    }

    screen.status("Done, restarting");
    delay.delay_ms(1000);
    cortex_m::peripheral::SCB::sys_reset();
}

/// The old system's vector table may be about to be erased, so interrupts and faults must go through a copy in RAM.
fn own_the_vector_table() {
    cortex_m::interrupt::disable();
    extern "C" {
        static __vector_table: u32;
    }
    unsafe {
        let source = core::ptr::addr_of!(__vector_table);
        for index in 0..VECTOR_COUNT {
            VECTOR_TABLE_IN_RAM
                .add(index)
                .write_volatile(source.add(index).read_volatile());
        }
        (*cortex_m::peripheral::SCB::PTR)
            .vtor
            .write(VECTOR_TABLE_IN_RAM as u32);
    }
    cortex_m::asm::dsb();
    cortex_m::asm::isb();
}

/// The old bootloader's watchdog keeps running; it cannot be stopped, only fed.
fn feed_watchdog() {
    unsafe { WATCHDOG_RELOAD_REGISTER_0.write_volatile(WATCHDOG_RELOAD) };
}

/// Holding the button lets the watchdog reset the watch: into the old update mode before the bootloader write, into BattleTime's bootloader after it.
fn feed_watchdog_while_button_is_up(button: &Input<'_>) {
    if button.is_low() {
        feed_watchdog();
    }
}

/// Stops here for good, until the button is held.
fn halt(button: &Input<'_>) -> ! {
    loop {
        feed_watchdog_while_button_is_up(button);
        cortex_m::asm::delay(CPU_FREQUENCY_IN_HERTZ / 10);
    }
}

/// The charger's two pins; the voltage comes from the SAADC.
struct Battery<'d> {
    power_present: Input<'d>,
    charging: Input<'d>,
}

impl Battery<'_> {
    /// One reading of the battery and the charger, or none when the converter never answered.
    fn measure(&self) -> Option<Measurement> {
        Some(Measurement {
            millivolts: battletime_battery_reading::read_millivolts(&mut battletime_battery_reading::Saadc)?,
            is_power_present: self.power_present.is_low(),
            is_charge_pin_active: self.charging.is_low(),
        })
    }
}

/// Polls between battery readings while waiting for the charger: one a second.
const POLLS_PER_BATTERY_READING: u32 = 10;

/// Waits, showing the battery each second, until the watch is on the charger with enough charge to write the bootloader.
fn wait_until_the_battery_is_safe(
    battery: &Battery<'_>,
    button: &Input<'_>,
    screen: &mut Screen,
    delay: &mut Delay,
) {
    let mut shown_reason = None;
    loop {
        let measurement = battery.measure();
        screen.battery(measurement);
        let reason = match measurement {
            Some(measurement) if can_write_bootloader(measurement) => return,
            Some(measurement) if !measurement.is_power_present => "Plug in the charger",
            Some(_) => "Battery low, charging",
            None => "Battery unreadable",
        };
        if shown_reason != Some(reason) {
            screen.status(reason);
            shown_reason = Some(reason);
        }
        for _ in 0..POLLS_PER_BATTERY_READING {
            feed_watchdog_while_button_is_up(button);
            delay.delay_ms(100);
        }
    }
}

fn payloads_are_intact() -> bool {
    let mut intact = crc32(CRC32_INITIAL, BOOTLOADER.data()) == BOOTLOADER.crc32
        && crc32(CRC32_INITIAL, FIRMWARE.data()) == FIRMWARE.crc32;
    if let Some(recovery) = RECOVERY.as_ref() {
        intact &= crc32(CRC32_INITIAL, recovery.data()) == recovery.crc32;
    }
    feed_watchdog();
    intact
}

// ============================================================================
// Copying into flash
// ============================================================================

/// The SPI driver can send at most 255 bytes straight from flash, so every payload chunk goes through RAM first.
struct RamChunk {
    bytes: [u8; COPY_CHUNK_SIZE],
}

fn into_ram(chunk: &[u8]) -> RamChunk {
    let mut ram_chunk = RamChunk {
        bytes: [0xFF; COPY_CHUNK_SIZE],
    };
    ram_chunk.bytes[..chunk.len()].copy_from_slice(chunk);
    ram_chunk
}

/// Erases the region, writes the payload a chunk at a time, and reads it back; tries again on a mismatch.
fn copy_with_retries<Flash: NorFlash>(flash: &mut Flash, payload: &Payload, screen: &mut Screen) -> bool {
    for _ in 0..WRITE_RETRIES {
        if copy(flash, payload, screen).is_ok() && read_back_matches(flash, payload) {
            return true;
        }
    }
    false
}

fn copy<Flash: NorFlash>(
    flash: &mut Flash,
    payload: &Payload,
    screen: &mut Screen,
) -> Result<(), Flash::Error> {
    let padded = payload.padded();
    let erase_end = (padded.len() as u32).div_ceil(PAGE_SIZE) * PAGE_SIZE;
    feed_watchdog();
    flash.erase(0, erase_end)?;

    for (index, chunk) in padded.chunks(COPY_CHUNK_SIZE).enumerate() {
        feed_watchdog();
        let ram_chunk = into_ram(chunk);
        flash.write((index * COPY_CHUNK_SIZE) as u32, &ram_chunk.bytes[..chunk.len()])?;
        screen.progress(index * COPY_CHUNK_SIZE, padded.len());
    }
    Ok(())
}

fn read_back_matches<Flash: ReadNorFlash>(flash: &mut Flash, payload: &Payload) -> bool {
    let mut chunk = [0; COPY_CHUNK_SIZE];
    let mut crc = CRC32_INITIAL;
    let mut offset = 0;
    while offset < payload.length_in_bytes {
        let length = (payload.length_in_bytes - offset).min(COPY_CHUNK_SIZE);
        feed_watchdog();
        if flash.read(offset as u32, &mut chunk[..length]).is_err() {
            return false;
        }
        crc = crc32(crc, &chunk[..length]);
        offset += length;
    }
    crc == payload.crc32
}

/// The firmware goes through embassy-boot's own updater, so it lands exactly as a Bluetooth update would.
fn stage_firmware<External: NorFlash, Internal: NorFlash>(
    external_flash: &Mutex<NoopRawMutex, RefCell<External>>,
    internal_flash: &Mutex<NoopRawMutex, RefCell<Internal>>,
    screen: &mut Screen,
) -> bool {
    let mut alignment = AlignedBuffer([0; 4]);
    let mut updater = BlockingFirmwareUpdater::new(
        FirmwareUpdaterConfig {
            dfu: BlockingPartition::new(
                external_flash,
                DEFAULT_SLOTS.update.start,
                DEFAULT_SLOTS.update.size,
            ),
            state: BlockingPartition::new(internal_flash, BOOT_STATE.start, BOOT_STATE.size),
        },
        &mut alignment.0,
    );

    for _ in 0..WRITE_RETRIES {
        let mut written = true;
        for (index, chunk) in FIRMWARE.padded().chunks(COPY_CHUNK_SIZE).enumerate() {
            feed_watchdog();
            let ram_chunk = into_ram(chunk);
            if updater
                .write_firmware(index * COPY_CHUNK_SIZE, &ram_chunk.bytes[..chunk.len()])
                .is_err()
            {
                written = false;
                break;
            }
            screen.progress(index * COPY_CHUNK_SIZE, FIRMWARE.padded().len());
        }
        let mut slot = BlockingPartition::new(
            external_flash,
            DEFAULT_SLOTS.update.start,
            DEFAULT_SLOTS.update.size,
        );
        if written && read_back_matches(&mut slot, &FIRMWARE) {
            return true;
        }
    }
    false
}

fn mark_swap<External: NorFlash, Internal: NorFlash>(
    external_flash: &Mutex<NoopRawMutex, RefCell<External>>,
    internal_flash: &Mutex<NoopRawMutex, RefCell<Internal>>,
) -> bool {
    let mut alignment = AlignedBuffer([0; 4]);
    let mut updater = BlockingFirmwareUpdater::new(
        FirmwareUpdaterConfig {
            dfu: BlockingPartition::new(
                external_flash,
                DEFAULT_SLOTS.update.start,
                DEFAULT_SLOTS.update.size,
            ),
            state: BlockingPartition::new(internal_flash, BOOT_STATE.start, BOOT_STATE.size),
        },
        &mut alignment.0,
    );
    feed_watchdog();
    updater.mark_updated().is_ok()
}

// ============================================================================
// Feeding the watchdog through long flash work
// ============================================================================

/// A sector erase can take hundreds of milliseconds at worst, so the watchdog is fed around every one.
struct FeedsWatchdog<Flash> {
    flash: Flash,
}

impl<Flash: ErrorType> ErrorType for FeedsWatchdog<Flash> {
    type Error = Flash::Error;
}

impl<Flash: ReadNorFlash> ReadNorFlash for FeedsWatchdog<Flash> {
    const READ_SIZE: usize = Flash::READ_SIZE;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        feed_watchdog();
        self.flash.read(offset, bytes)
    }

    fn capacity(&self) -> usize {
        self.flash.capacity()
    }
}

impl<Flash: NorFlash> NorFlash for FeedsWatchdog<Flash> {
    const WRITE_SIZE: usize = Flash::WRITE_SIZE;
    const ERASE_SIZE: usize = Flash::ERASE_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        for sector_start in (from..to).step_by(Flash::ERASE_SIZE) {
            feed_watchdog();
            self.flash
                .erase(sector_start, sector_start + Flash::ERASE_SIZE as u32)?;
        }
        feed_watchdog();
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        feed_watchdog();
        self.flash.write(offset, bytes)
    }
}

// ============================================================================
// The screen
// ============================================================================

type DisplayInterfaceOnBus = MipidsiInterface<
    SpiInterface<'static, SpiDevice<'static, NoopRawMutex, Spim<'static>, Output<'static>>, Output<'static>>,
>;

struct Screen {
    display: Display<DisplayInterfaceOnBus, Output<'static>>,
    last_progress_width: u32,
}

impl Screen {
    const TITLE_BASELINE: i32 = 40;
    const STATUS_BASELINE: i32 = 120;
    const STATUS_HEIGHT: u32 = 24;
    const BAR: Rectangle = Rectangle::new(Point::new(20, 160), Size::new(200, 16));
    const BATTERY_BASELINE: i32 = 215;

    fn start(
        spi_bus: &'static SpiBus,
        chip_select: Output<'static>,
        data_or_command: Output<'static>,
        reset: Output<'static>,
        delay: &mut Delay,
    ) -> Option<Screen> {
        static COMMAND_BUFFER: StaticCell<[u8; 512]> = StaticCell::new();
        let interface = MipidsiInterface(SpiInterface::new(
            SpiDevice::new(spi_bus, chip_select),
            data_or_command,
            COMMAND_BUFFER.init([0; 512]),
        ));
        let mut display = Display::new(interface, reset, delay).ok()?;
        let _ = display.clear(Rgb565::BLACK);
        let _ = Self::BAR
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
            .draw(&mut display);
        Some(Screen {
            display,
            last_progress_width: 0,
        })
    }

    fn title(&mut self, text: &str) {
        let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        let _ = Text::new(text, Point::new(12, Self::TITLE_BASELINE), style).draw(&mut self.display);
    }

    fn status(&mut self, text: &str) {
        let clear = Rectangle::new(
            Point::new(0, Self::STATUS_BASELINE - 20),
            Size::new(240, Self::STATUS_HEIGHT),
        );
        let _ = clear
            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
            .draw(&mut self.display);
        let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        let _ = Text::new(text, Point::new(12, Self::STATUS_BASELINE), style).draw(&mut self.display);
        self.progress(0, 1);
    }

    /// Shows the battery's charge and the charger, such as "Battery 87% charging", below the bar.
    fn battery(&mut self, measurement: Option<Measurement>) {
        let mut buffer = [0; BATTERY_LINE_LENGTH];
        let text = measurement.map_or("Battery unreadable", |measurement| {
            battery_line(measurement, &mut buffer)
        });
        let clear = Rectangle::new(
            Point::new(0, Self::BATTERY_BASELINE - 20),
            Size::new(240, Self::STATUS_HEIGHT),
        );
        let _ = clear
            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
            .draw(&mut self.display);
        let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        let _ = Text::new(text, Point::new(12, Self::BATTERY_BASELINE), style).draw(&mut self.display);
    }

    fn progress(&mut self, done: usize, total: usize) {
        let inner = Self::BAR.offset(-2);
        let width = (inner.size.width as usize * done / total.max(1)) as u32;
        if width == self.last_progress_width {
            return;
        }
        let (colour, rectangle) = if width < self.last_progress_width {
            (Rgb565::BLACK, inner)
        } else {
            (
                Rgb565::WHITE,
                Rectangle::new(inner.top_left, Size::new(width, inner.size.height)),
            )
        };
        let _ = rectangle
            .into_styled(PrimitiveStyle::with_fill(colour))
            .draw(&mut self.display);
        self.last_progress_width = width;
    }
}
