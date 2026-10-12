//! Replaces the bootloader on a watch already running NeoTime.
//!
//! It is sent as an ordinary firmware update, so the bootloader swaps it in on trial. It checks the
//! bootloader it carries, waits for the charger, writes the bootloader pages and reads them back. It
//! never confirms itself, so its reset makes the new bootloader roll back to the firmware before it,
//! and a note in RAM tells that firmware the rollback was meant. See docs/bootloader.md.

#![no_std]
#![no_main]

use cortex_m::delay::Delay;
use cortex_m_rt::entry;
use embassy_nrf::{
    bind_interrupts,
    gpio::{Input, Level, Output, OutputDrive, Pull},
    nvmc::Nvmc,
    peripherals,
    spim::{self, Spim},
    wdt::{self, HaltConfig, SleepConfig, Watchdog, WatchdogHandle},
};
use embedded_graphics::pixelcolor::{raw::RawU16, Rgb565};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use neotime_boot::{
    is_bootloader_vector_table,
    migration::{crc32, CRC32_INITIAL},
    status_colour, UpdaterNote, BOOTLOADER, PAGE_SIZE, UPDATER_NOTE_ADDRESS, VECTOR_TABLE_CHECK_SIZE,
    WATCHDOG_TIMEOUT_IN_TICKS,
};
use neotime_status_screen::Screen;
// A panic halts, so the watchdog resets the watch; before the write that rolls the firmware back.
use panic_halt as _;

mod new_bootloader {
    include!(concat!(env!("OUT_DIR"), "/new_bootloader.rs"));
}

// ============================================================================
// PineTime wiring and timing
// ============================================================================

const CPU_FREQUENCY_IN_HERTZ: u32 = 64_000_000;
const POLL_INTERVAL_IN_MILLISECONDS: u32 = 100;
const COPY_CHUNK_SIZE: usize = 1024;
const WRITE_ATTEMPTS_BEFORE_WARNING: u32 = 3;

bind_interrupts!(struct Interrupts {
    TWISPI0 => spim::InterruptHandler<peripherals::TWISPI0>;
});

// ============================================================================
// Start
// ============================================================================

#[entry]
fn main() -> ! {
    let mut config = embassy_nrf::config::Config::default();
    // Leave the debug port's settings alone, so start-up never writes the protection word.
    config.debug = embassy_nrf::config::Debug::NotConfigured;
    let chip = embassy_nrf::init(config);
    let core = cortex_m::Peripherals::take().unwrap();
    let mut delay = Delay::new(core.SYST, CPU_FREQUENCY_IN_HERTZ);
    let mut watchdog = take_over_watchdog(chip.WDT);

    // The button reads high while held, and only while its power pin is driven high.
    let _button_power = Output::new(chip.P0_15, Level::High, OutputDrive::Standard);
    let button = Input::new(chip.P0_13, Pull::Down);
    let power_present = Input::new(chip.P0_19, Pull::Up);
    let _flash_kept_off_the_bus = Output::new(chip.P0_05, Level::High, OutputDrive::Standard);

    let mut spi_config = spim::Config::default();
    spi_config.frequency = spim::Frequency::M8;
    let spi_bus = Spim::new(
        chip.TWISPI0,
        Interrupts,
        chip.P0_02,
        chip.P0_04,
        chip.P0_03,
        spi_config,
    );
    let display_chip_select = Output::new(chip.P0_25, Level::High, OutputDrive::Standard);
    let Ok(display_spi) = ExclusiveDevice::new_no_delay(spi_bus, display_chip_select);
    // Kept driven for the whole run: a dropped pin floats, and a floating reset blanks the panel.
    let mut display_reset = Output::new(chip.P0_26, Level::High, OutputDrive::Standard);
    let screen = Screen::start(
        display_spi,
        Output::new(chip.P0_18, Level::Low, OutputDrive::Standard),
        &mut display_reset,
        Rgb565::from(RawU16::new(status_colour::BOOTLOADER)),
        &mut delay,
    );
    let _backlight_on = Output::new(chip.P0_14, Level::Low, OutputDrive::Standard);
    let mut status = Status { screen };
    status.show(&["Bootloader update", "Checking", ""]);

    if !carried_bootloader_is_intact() {
        status.show(&["Bootloader update", "Damaged download", "Hold button to quit"]);
        wait_for_the_button_hold(&button, &mut watchdog, &mut delay);
    }

    let mut internal_flash = Nvmc::new(chip.NVMC);
    if installed_bootloader_matches(&mut internal_flash) {
        status.show(&["Bootloader update", "Already up to date", "Restarting"]);
        hand_back(UpdaterNote::BootloaderAlreadyCurrent, &mut watchdog, &mut delay);
    }

    let mut asked_for_the_charger = false;
    while power_present.is_high() {
        if !asked_for_the_charger {
            status.show(&["Plug in the charger", "Hold button to quit", ""]);
            asked_for_the_charger = true;
        }
        feed_watchdog_while_button_is_up(&button, &mut watchdog);
        delay.delay_ms(POLL_INTERVAL_IN_MILLISECONDS);
    }

    // From the first erase until the read-back matches, a reset would leave nothing that starts, so
    // the watchdog is fed whatever the button does and the updater never gives up.
    status.show(&["Bootloader update", "Installing", "Do not unplug"]);
    let mut attempts = 0;
    while !write_and_check(&mut internal_flash, &mut watchdog) {
        attempts += 1;
        if attempts == WRITE_ATTEMPTS_BEFORE_WARNING {
            status.show(&["Write failed", "Trying again", "Do not unplug"]);
        }
    }

    status.show(&["Bootloader update", "Done, restarting", ""]);
    hand_back(UpdaterNote::BootloaderWritten, &mut watchdog, &mut delay)
}

// ============================================================================
// The watchdog
// ============================================================================

/// The bootloader started it and it cannot be stopped, so take it over with the same settings.
fn take_over_watchdog(watchdog_peripheral: embassy_nrf::Peri<'static, peripherals::WDT>) -> WatchdogHandle {
    let mut config = wdt::Config::default();
    config.timeout_ticks = WATCHDOG_TIMEOUT_IN_TICKS;
    config.action_during_sleep = SleepConfig::Run;
    config.action_during_debug_halt = HaltConfig::Pause;

    match Watchdog::try_new(watchdog_peripheral, config) {
        Ok((_watchdog, [handle])) => handle,
        Err(_) => loop {
            cortex_m::asm::wfe();
        },
    }
}

/// Holding the button lets the watchdog reset the watch, and the bootloader then rolls the firmware back.
fn feed_watchdog_while_button_is_up(button: &Input<'_>, watchdog: &mut WatchdogHandle) {
    if button.is_low() {
        watchdog.pet();
    }
}

fn wait_for_the_button_hold(button: &Input<'_>, watchdog: &mut WatchdogHandle, delay: &mut Delay) -> ! {
    loop {
        feed_watchdog_while_button_is_up(button, watchdog);
        delay.delay_ms(POLL_INTERVAL_IN_MILLISECONDS);
    }
}

/// Leaves the note and resets; the updater never confirmed itself, so the bootloader puts the previous firmware back.
fn hand_back(note: UpdaterNote, watchdog: &mut WatchdogHandle, delay: &mut Delay) -> ! {
    unsafe { (UPDATER_NOTE_ADDRESS as *mut u32).write_volatile(note.to_word()) };
    watchdog.pet();
    delay.delay_ms(1000);
    cortex_m::peripheral::SCB::sys_reset()
}

// ============================================================================
// Checking and writing the bootloader
// ============================================================================

fn carried_bootloader_is_intact() -> bool {
    let image = &new_bootloader::PADDED[..new_bootloader::LENGTH_IN_BYTES];
    let vector_table_start: [u8; VECTOR_TABLE_CHECK_SIZE] =
        image[..VECTOR_TABLE_CHECK_SIZE].try_into().unwrap();
    crc32(CRC32_INITIAL, image) == new_bootloader::CRC32 && is_bootloader_vector_table(vector_table_start)
}

fn installed_bootloader_matches(internal_flash: &mut Nvmc<'_>) -> bool {
    installed_crc(internal_flash) == Some(new_bootloader::CRC32)
}

/// The CRC of the installed bootloader over the carried one's length, read a chunk at a time.
fn installed_crc(internal_flash: &mut Nvmc<'_>) -> Option<u32> {
    let mut chunk = [0; COPY_CHUNK_SIZE];
    let mut crc = CRC32_INITIAL;
    let mut offset = 0;
    while offset < new_bootloader::LENGTH_IN_BYTES {
        let length = (new_bootloader::LENGTH_IN_BYTES - offset).min(COPY_CHUNK_SIZE);
        internal_flash
            .read(BOOTLOADER.start + offset as u32, &mut chunk[..length])
            .ok()?;
        crc = crc32(crc, &chunk[..length]);
        offset += length;
    }
    Some(crc)
}

/// Erases the pages the new bootloader covers, writes it from RAM a chunk at a time, and reads it back.
fn write_and_check(internal_flash: &mut Nvmc<'_>, watchdog: &mut WatchdogHandle) -> bool {
    let padded = new_bootloader::PADDED;
    let pages_end = BOOTLOADER.start + (padded.len() as u32).div_ceil(PAGE_SIZE) * PAGE_SIZE;
    for page_start in (BOOTLOADER.start..pages_end).step_by(PAGE_SIZE as usize) {
        watchdog.pet();
        if internal_flash.erase(page_start, page_start + PAGE_SIZE).is_err() {
            return false;
        }
    }

    for (index, chunk) in padded.chunks(COPY_CHUNK_SIZE).enumerate() {
        watchdog.pet();
        let mut in_ram = [0xFF; COPY_CHUNK_SIZE];
        in_ram[..chunk.len()].copy_from_slice(chunk);
        let offset = BOOTLOADER.start + (index * COPY_CHUNK_SIZE) as u32;
        if internal_flash.write(offset, &in_ram[..chunk.len()]).is_err() {
            return false;
        }
    }

    watchdog.pet();
    installed_bootloader_matches(internal_flash)
}

// ============================================================================
// The screen
// ============================================================================

type StatusScreen<'d> =
    Screen<ExclusiveDevice<Spim<'d>, Output<'d>, embedded_hal_bus::spi::NoDelay>, Output<'d>>;

/// The status screen, if the panel started; the update goes ahead without it.
struct Status<'d> {
    screen: Option<StatusScreen<'d>>,
}

impl Status<'_> {
    fn show(&mut self, lines: &[&str; 3]) {
        if let Some(screen) = &mut self.screen {
            for (index, text) in lines.iter().enumerate() {
                screen.write_line(index as i32, text);
            }
        }
    }
}
