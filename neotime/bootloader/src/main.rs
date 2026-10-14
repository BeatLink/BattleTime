//! NeoTime's bootloader: swaps in a waiting update or the recovery firmware, then starts the firmware.
//!
//! Holding the button for five seconds at power-on opens a menu: start the firmware, roll back to
//! the previous one, or install the recovery firmware from external flash. A firmware that does not mark itself booted before its first reset is rolled back,
//! and one without a sound image header is replaced by the previous or the recovery firmware. The
//! screen stays dark on an ordinary start and turns blue with a few words while the bootloader works.

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::{delay::Delay, peripheral::NVIC};
use cortex_m_rt::entry;
use embassy_boot::State;
use embassy_boot_nrf::{AlignedBuffer, BlockingFirmwareState, BootLoader, BootLoaderConfig};
use embassy_embedded_hal::flash::partition::BlockingPartition;
use embassy_nrf::{
    bind_interrupts,
    gpio::{Input, Level, Output, OutputDrive, Pull},
    nvmc::Nvmc,
    peripherals::{
        self, P0_02, P0_03, P0_04, P0_05, P0_13, P0_14, P0_15, P0_16, P0_18, P0_25, P0_26, TWISPI0,
    },
    spim::{self, Spim},
    wdt::{self, HaltConfig, SleepConfig, Watchdog, WatchdogHandle},
    Peri,
};
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, Mutex};
use embedded_graphics::pixelcolor::{raw::RawU16, Rgb565};
use embedded_hal_bus::spi::RefCellDevice;
use embedded_storage::nor_flash::{ErrorType, NorFlash, ReadNorFlash};
use neotime_boot::{
    copy_image_into_update_slot, copy_previous_into_update_slot, firmware_is_runnable,
    image_header::installable_image, slot_table::read_slots, status_colour, BootNote, ImageCopy, Slots,
    BOOT_NOTE_ADDRESS, BOOT_STATE, DEFAULT_SLOTS, FIRMWARE, FIRMWARE_VECTOR_TABLE,
    MENU_BUTTON_HOLD_IN_MILLISECONDS, MENU_SELECT_HOLD_IN_MILLISECONDS, MENU_TIMEOUT_IN_MILLISECONDS,
    PAGE_SIZE, SLOT_TABLE, WATCHDOG_TIMEOUT_IN_TICKS,
};
use neotime_button::Button;
use neotime_external_flash::{BusyWait, ExternalFlash};
use neotime_vibrator::{OnOffMotor, Strength, Vibrator};
use panic_reset as _;

use neotime_status_screen::{page, Screen};

// ============================================================================
// PineTime wiring
// ============================================================================

const CPU_FREQUENCY_IN_HERTZ: u32 = 64_000_000;

struct Pins<'a> {
    button_input: Peri<'a, P0_13>,
    button_enable: Peri<'a, P0_15>,
    motor: Peri<'a, P0_16>,
    spi_clock: Peri<'a, P0_02>,
    spi_data_out: Peri<'a, P0_03>,
    spi_data_in: Peri<'a, P0_04>,
    flash_chip_select: Peri<'a, P0_05>,
    display_chip_select: Peri<'a, P0_25>,
    display_data_or_command: Peri<'a, P0_18>,
    display_reset: Peri<'a, P0_26>,
    backlight: Peri<'a, P0_14>,
}

bind_interrupts!(struct Interrupts {
    TWISPI0 => spim::InterruptHandler<peripherals::TWISPI0>;
});

// ============================================================================
// Start
// ============================================================================

#[entry]
fn main() -> ! {
    let chip = embassy_nrf::init(Default::default());
    let core = cortex_m::Peripherals::take().unwrap();
    let mut delay = Delay::new(core.SYST, CPU_FREQUENCY_IN_HERTZ);

    let watchdog = RefCell::new(start_watchdog(chip.WDT));
    let pins = Pins {
        button_input: chip.P0_13,
        button_enable: chip.P0_15,
        motor: chip.P0_16,
        spi_clock: chip.P0_02,
        spi_data_out: chip.P0_03,
        spi_data_in: chip.P0_04,
        flash_chip_select: chip.P0_05,
        display_chip_select: chip.P0_25,
        display_data_or_command: chip.P0_18,
        display_reset: chip.P0_26,
        backlight: chip.P0_14,
    };

    let note = prepare_firmware(pins, chip.TWISPI0, chip.NVMC, &watchdog, &mut delay);
    leave_note(note);
    hand_over_to_firmware()
}

/// Keeps a watchdog left running by the firmware, since it cannot be stopped short of a reset.
fn start_watchdog(watchdog_peripheral: Peri<'static, peripherals::WDT>) -> WatchdogHandle {
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

// ============================================================================
// Preparing the firmware
// ============================================================================

fn prepare_firmware<'d>(
    pins: Pins<'d>,
    spi_peripheral: Peri<'d, TWISPI0>,
    flash_controller: Peri<'_, peripherals::NVMC>,
    watchdog: &RefCell<WatchdogHandle>,
    delay: &mut Delay,
) -> BootNote {
    let mut spi_config = spim::Config::default();
    spi_config.frequency = spim::Frequency::M8;
    let spi_bus = RefCell::new(Spim::new(
        spi_peripheral,
        Interrupts,
        pins.spi_clock,
        pins.spi_data_in,
        pins.spi_data_out,
        spi_config,
    ));
    let display_chip_select = Output::new(pins.display_chip_select, Level::High, OutputDrive::Standard);
    let flash_chip_select = Output::new(pins.flash_chip_select, Level::High, OutputDrive::Standard);
    let mut status_page = StatusPage {
        spi_bus: &spi_bus,
        chip_select: Some(display_chip_select),
        data_or_command: Some(pins.display_data_or_command),
        reset: Some(pins.display_reset),
        backlight: Some(pins.backlight),
        screen: None,
    };

    let button_pin = Input::new(pins.button_input, Pull::Down);
    let power_pin = Output::new(pins.button_enable, Level::Low, OutputDrive::Standard);
    let Ok(mut button) = Button::new(button_pin, power_pin);
    delay.delay_ms(BUTTON_SETTLE_TIME_IN_MILLISECONDS);
    let menu_requested = button.is_pressed() == Ok(true) && {
        status_page.show("Unchanged", "Keep holding for", "the menu", delay);
        button_held_for(MENU_BUTTON_HOLD_IN_MILLISECONDS, &mut button, watchdog, delay)
    };
    let mut vibrator = Vibrator::new(OnOffMotor {
        motor_pin: Output::new(pins.motor, Level::High, OutputDrive::Standard),
    })
    .unwrap();
    let choice = if menu_requested {
        buzz(&mut vibrator, 200, delay);
        choose(&mut status_page, &mut button, &mut vibrator, watchdog, delay)
    } else {
        Choice::StartFirmware
    };

    let Ok(flash_spi) = RefCellDevice::new_no_delay(&spi_bus, flash_chip_select);
    let Ok(external_flash) = ExternalFlash::new(
        flash_spi,
        BusyWait {
            cpu_frequency_in_hertz: CPU_FREQUENCY_IN_HERTZ,
        },
    ) else {
        return BootNote::Normal;
    };

    let internal_flash = Mutex::<NoopRawMutex, _>::new(RefCell::new(PetsWatchdog {
        flash: Nvmc::new(flash_controller),
        watchdog,
    }));
    let external_flash = Mutex::<NoopRawMutex, _>::new(RefCell::new(PetsWatchdog {
        flash: external_flash,
        watchdog,
    }));

    let slots = read_slots(&mut BlockingPartition::new(
        &internal_flash,
        SLOT_TABLE.start,
        SLOT_TABLE.size,
    ))
    .unwrap_or(DEFAULT_SLOTS);

    let mut recovery_installed = false;
    let source = match choice {
        Choice::StartFirmware => None,
        Choice::RollBack => Some(Source::Previous),
        Choice::InstallRecovery => Some(Source::Recovery),
    };
    if let Some(source) = source {
        let (doing, missing) = match source {
            Source::Previous => ("Rolling back", "No previous firmware"),
            Source::Recovery => ("Installing recovery", "No recovery found"),
        };
        status_page.show("Swapping", doing, "Do not unplug", delay);
        let marked = mark_install(source, slots, &internal_flash, &external_flash);
        recovery_installed = marked && source == Source::Recovery;

        if !marked {
            status_page.show("Unchanged", missing, "Starting firmware", delay);
            for _ in 0..3 {
                buzz(&mut vibrator, 60, delay);
                delay.delay_ms(120);
            }
        }
    } else if boot_state_is(&internal_flash, State::Swap) {
        status_page.show("Swapping", "Installing firmware", "Do not unplug", delay);
    }

    prepare(slots, &internal_flash, &external_flash);

    let runnable = internal_flash
        .lock(|flash| firmware_is_runnable(&mut *flash.borrow_mut()))
        .unwrap_or(false);
    if !runnable {
        status_page.show("Damaged", "Repairing firmware", "Do not unplug", delay);
        let repaired_by = [Source::Previous, Source::Recovery]
            .into_iter()
            .find(|&source| mark_install(source, slots, &internal_flash, &external_flash));
        if repaired_by.is_none() {
            // Nothing to install: wait for the watchdog, which tries again from the start.
            status_page.show("Damaged", "No firmware found", "Send an update", delay);
            loop {
                cortex_m::asm::wfe();
            }
        }
        prepare(slots, &internal_flash, &external_flash);
        recovery_installed = repaired_by == Some(Source::Recovery);
    }

    if recovery_installed {
        BootNote::RecoveryInstalled
    } else {
        BootNote::Normal
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Previous,
    Recovery,
}

/// Puts the previous or the recovery firmware in the update slot and marks a swap; false when it has no whole image.
///
/// Clearing the state first drops a pending swap's progress, so the copy is installed by a fresh swap;
/// a revert would read the slot the copy has just overwritten.
fn mark_install<InternalFlash: NorFlash, ExternalFlash: NorFlash>(
    source: Source,
    slots: Slots,
    internal_flash: &Mutex<NoopRawMutex, RefCell<InternalFlash>>,
    external_flash: &Mutex<NoopRawMutex, RefCell<ExternalFlash>>,
) -> bool {
    let region = match source {
        Source::Previous => slots.previous_firmware(),
        Source::Recovery => slots.recovery,
    };
    let has_image = external_flash
        .lock(|flash| installable_image(&mut *flash.borrow_mut(), region.start, region.size))
        .is_ok_and(|header| header.is_some());
    if !has_image {
        return false;
    }

    let mut state_alignment = AlignedBuffer([0; 4]);
    let mut boot_state = BlockingFirmwareState::new(
        BlockingPartition::new(internal_flash, BOOT_STATE.start, BOOT_STATE.size),
        &mut state_alignment.0,
    );
    let copied = boot_state.mark_booted().is_ok()
        && external_flash.lock(|flash| {
            let flash = &mut *flash.borrow_mut();
            let copy = match source {
                Source::Previous => copy_previous_into_update_slot(flash, slots),
                Source::Recovery => copy_image_into_update_slot(flash, slots, slots.recovery),
            };
            matches!(copy, Ok(ImageCopy::Copied))
        });
    copied && boot_state.mark_updated().is_ok()
}

/// Lets embassy-boot swap, roll back or do nothing, as `BOOT_STATE` says.
fn prepare<InternalFlash: NorFlash, ExternalFlash: NorFlash>(
    slots: Slots,
    internal_flash: &Mutex<NoopRawMutex, RefCell<InternalFlash>>,
    external_flash: &Mutex<NoopRawMutex, RefCell<ExternalFlash>>,
) {
    let config = BootLoaderConfig {
        active: BlockingPartition::new(internal_flash, FIRMWARE.start, FIRMWARE.size),
        dfu: BlockingPartition::new(external_flash, slots.update.start, slots.update.size),
        state: BlockingPartition::new(internal_flash, BOOT_STATE.start, BOOT_STATE.size),
    };
    let _ = BootLoader::<{ PAGE_SIZE as usize }>::try_prepare(config);
}

fn boot_state_is<Flash: NorFlash>(
    internal_flash: &Mutex<NoopRawMutex, RefCell<Flash>>,
    state: State,
) -> bool {
    let mut state_alignment = AlignedBuffer([0; 4]);
    let mut boot_state = BlockingFirmwareState::new(
        BlockingPartition::new(internal_flash, BOOT_STATE.start, BOOT_STATE.size),
        &mut state_alignment.0,
    );
    boot_state
        .get_state()
        .map(|current| current == state)
        .unwrap_or(false)
}

/// Tells the firmware why it was started this time, in RAM the bootloader's own linker keeps free.
fn leave_note(note: BootNote) {
    unsafe { (BOOT_NOTE_ADDRESS as *mut u32).write_volatile(note.to_word()) };
}

// ============================================================================
// The status page, started only when there is something to say
// ============================================================================

const BUTTON_SETTLE_TIME_IN_MILLISECONDS: u32 = 1;

type StatusScreen<'bus, 'd> =
    Screen<RefCellDevice<'bus, Spim<'d>, Output<'d>, embedded_hal_bus::spi::NoDelay>, Output<'d>>;

/// `'bus` is the borrow of the shared SPI bus, `'d` the pins and peripherals it was built from.
struct StatusPage<'bus, 'd> {
    spi_bus: &'bus RefCell<Spim<'d>>,
    chip_select: Option<Output<'d>>,
    data_or_command: Option<Peri<'d, P0_18>>,
    reset: Option<Peri<'d, P0_26>>,
    backlight: Option<Peri<'d, P0_14>>,
    /// The screen, then the backlight and reset pins, kept driven: a dropped pin floats, and a floating reset blanks the panel.
    screen: Option<(StatusScreen<'bus, 'd>, Output<'d>, Output<'d>)>,
}

impl<'bus, 'd> StatusPage<'bus, 'd> {
    /// Starts the panel and fills the lines that never change the first time, then shows the firmware's
    /// state, the bootloader's work and what the wearer should know.
    fn show(&mut self, firmware: &str, action: &str, progress: &str, delay: &mut Delay) {
        if self.screen.is_none() {
            let (Some(chip_select), Some(data_or_command), Some(reset), Some(backlight)) = (
                self.chip_select.take(),
                self.data_or_command.take(),
                self.reset.take(),
                self.backlight.take(),
            ) else {
                return;
            };
            let Ok(spi) = RefCellDevice::new_no_delay(self.spi_bus, chip_select);
            let background = Rgb565::from(RawU16::new(status_colour::BOOTLOADER));
            let mut reset_pin = Output::new(reset, Level::High, OutputDrive::Standard);
            let Some(screen) = Screen::start(
                spi,
                Output::new(data_or_command, Level::Low, OutputDrive::Standard),
                &mut reset_pin,
                background,
                delay,
            ) else {
                return;
            };
            let backlight_on = Output::new(backlight, Level::Low, OutputDrive::Standard);
            let mut screen = screen;
            screen.write_line(page::TITLE, "NeoTime bootloader");
            screen.write_line(page::BOOTLOADER, "Working");
            screen.write_line(page::FILES, "Not mounted");
            screen.write_line(page::BLUETOOTH, "Off");
            self.screen = Some((screen, backlight_on, reset_pin));
        }
        if let Some((screen, _, _)) = &mut self.screen {
            screen.write_line(page::FIRMWARE, firmware);
            screen.write_line(page::ACTION, action);
            screen.write_line(page::PROGRESS, progress);
            screen.write_line(page::BUILD, "");
            screen.write_line(page::LAST_RUN, "");
        }
    }

    /// Lists the options from the action line down, marks the selected one, and says how to use them.
    fn show_menu(&mut self, selected: usize) {
        if let Some((screen, _, _)) = &mut self.screen {
            for (index, (_, marked, unmarked)) in MENU.iter().enumerate() {
                screen.write_line(
                    page::ACTION + index,
                    if index == selected { marked } else { unmarked },
                );
            }
            screen.write_line(page::LAST_RUN, "Press: next Hold: pick");
        }
    }
}

// ============================================================================
// The menu: a press moves to the next option, a long press picks it
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    StartFirmware,
    RollBack,
    InstallRecovery,
}

/// Each option, and its line with and without the mark that shows it is the one a long press picks.
const MENU: [(Choice, &str, &str); 3] = [
    (Choice::StartFirmware, "> Start firmware", "  Start firmware"),
    (Choice::RollBack, "> Roll back", "  Roll back"),
    (
        Choice::InstallRecovery,
        "> Install recovery",
        "  Install recovery",
    ),
];

const MENU_POLL_INTERVAL_IN_MILLISECONDS: u32 = 10;

/// Shows the menu until a long press picks an option, or until it has waited too long, which starts the firmware.
fn choose(
    status_page: &mut StatusPage<'_, '_>,
    button: &mut Button<Input<'_>, Output<'_>>,
    vibrator: &mut Vibrator<OnOffMotor<Output<'_>>>,
    watchdog: &RefCell<WatchdogHandle>,
    delay: &mut Delay,
) -> Choice {
    // The hold that opened the menu is still going; it must end before a press counts.
    wait_for_release(button, watchdog, delay);
    status_page.show("Unchanged", "", "", delay);
    let mut selected = 0;
    loop {
        status_page.show_menu(selected);
        let mut waited = 0;
        while button.is_pressed() != Ok(true) {
            if waited >= MENU_TIMEOUT_IN_MILLISECONDS {
                return Choice::StartFirmware;
            }
            watchdog.borrow_mut().pet();
            delay.delay_ms(MENU_POLL_INTERVAL_IN_MILLISECONDS);
            waited += MENU_POLL_INTERVAL_IN_MILLISECONDS;
        }
        if button_held_for(MENU_SELECT_HOLD_IN_MILLISECONDS, button, watchdog, delay) {
            buzz(vibrator, 200, delay);
            wait_for_release(button, watchdog, delay);
            return MENU[selected].0;
        }
        selected = (selected + 1) % MENU.len();
    }
}

fn wait_for_release(
    button: &mut Button<Input<'_>, Output<'_>>,
    watchdog: &RefCell<WatchdogHandle>,
    delay: &mut Delay,
) {
    while button.is_pressed() == Ok(true) {
        watchdog.borrow_mut().pet();
        delay.delay_ms(MENU_POLL_INTERVAL_IN_MILLISECONDS);
    }
}

fn button_held_for(
    duration_in_milliseconds: u32,
    button: &mut Button<Input<'_>, Output<'_>>,
    watchdog: &RefCell<WatchdogHandle>,
    delay: &mut Delay,
) -> bool {
    for _ in 0..duration_in_milliseconds / MENU_POLL_INTERVAL_IN_MILLISECONDS {
        if button.is_pressed() != Ok(true) {
            return false;
        }
        watchdog.borrow_mut().pet();
        delay.delay_ms(MENU_POLL_INTERVAL_IN_MILLISECONDS);
    }
    true
}

fn buzz<MotorPin: embedded_hal::digital::OutputPin>(
    vibrator: &mut Vibrator<OnOffMotor<MotorPin>>,
    duration_in_milliseconds: u32,
    delay: &mut Delay,
) {
    let _ = vibrator.turn_on(Strength::FULL);
    delay.delay_ms(duration_in_milliseconds);
    let _ = vibrator.turn_off();
}

// ============================================================================
// Handing over
// ============================================================================

fn hand_over_to_firmware() -> ! {
    const INTERRUPT_REGISTER_COUNT: usize = 8;

    let interrupt_controller = unsafe { &*NVIC::PTR };
    for register in 0..INTERRUPT_REGISTER_COUNT {
        unsafe {
            interrupt_controller.icer[register].write(u32::MAX);
            interrupt_controller.icpr[register].write(u32::MAX);
        }
    }

    unsafe { BootLoader::<{ PAGE_SIZE as usize }>.load(FIRMWARE_VECTOR_TABLE) }
}

// ============================================================================
// Petting the watchdog during long flash work
// ============================================================================

struct PetsWatchdog<'a, Flash> {
    flash: Flash,
    watchdog: &'a RefCell<WatchdogHandle>,
}

impl<Flash: ErrorType> ErrorType for PetsWatchdog<'_, Flash> {
    type Error = Flash::Error;
}

impl<Flash: ReadNorFlash> ReadNorFlash for PetsWatchdog<'_, Flash> {
    const READ_SIZE: usize = Flash::READ_SIZE;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.watchdog.borrow_mut().pet();
        self.flash.read(offset, bytes)
    }

    fn capacity(&self) -> usize {
        self.flash.capacity()
    }
}

impl<Flash: NorFlash> NorFlash for PetsWatchdog<'_, Flash> {
    const WRITE_SIZE: usize = Flash::WRITE_SIZE;
    const ERASE_SIZE: usize = Flash::ERASE_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        for sector_start in (from..to).step_by(Flash::ERASE_SIZE) {
            self.watchdog.borrow_mut().pet();
            self.flash
                .erase(sector_start, sector_start + Flash::ERASE_SIZE as u32)?;
        }
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.watchdog.borrow_mut().pet();
        self.flash.write(offset, bytes)
    }
}
