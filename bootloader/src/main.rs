//! NeoTime's bootloader: swaps in a waiting update or the recovery firmware, then starts the firmware.
//!
//! Holding the button for five seconds at power-on installs the recovery firmware from external
//! flash. A firmware that does not mark itself booted before its first reset is rolled back.

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::{delay::Delay, peripheral::NVIC};
use cortex_m_rt::entry;
use embassy_boot_nrf::{AlignedBuffer, BlockingFirmwareState, BootLoader, BootLoaderConfig};
use embassy_embedded_hal::flash::partition::BlockingPartition;
use embassy_nrf::{
    bind_interrupts,
    gpio::{Input, Level, Output, OutputDrive, Pull},
    nvmc::Nvmc,
    peripherals::{self, P0_02, P0_03, P0_04, P0_05, P0_13, P0_15, P0_16, P0_25, TWISPI0},
    spim::{self, Spim},
    wdt::{self, HaltConfig, SleepConfig, Watchdog, WatchdogHandle},
    Peri,
};
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, Mutex};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_storage::nor_flash::{ErrorType, NorFlash, ReadNorFlash};
use neotime_boot::{
    copy_recovery_into_update_slot, RecoveryCopy, BOOT_STATE, FIRMWARE, FIRMWARE_UPDATE, PAGE_SIZE,
    RECOVERY_BUTTON_HOLD_IN_MILLISECONDS, WATCHDOG_TIMEOUT_IN_TICKS,
};
use neotime_button::Button;
use neotime_external_flash::ExternalFlash;
use neotime_vibrator::{OnOffMotor, Strength, Vibrator};
use panic_reset as _;

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
    };

    prepare_firmware(pins, chip.TWISPI0, chip.NVMC, &watchdog, &mut delay);
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

fn prepare_firmware(
    pins: Pins<'_>,
    spi_peripheral: Peri<'_, TWISPI0>,
    flash_controller: Peri<'_, peripherals::NVMC>,
    watchdog: &RefCell<WatchdogHandle>,
    delay: &mut Delay,
) {
    let recovery_requested = button_held_for(
        RECOVERY_BUTTON_HOLD_IN_MILLISECONDS,
        pins.button_input,
        pins.button_enable,
        watchdog,
        delay,
    );
    let mut vibrator = Vibrator::new(OnOffMotor {
        motor_pin: Output::new(pins.motor, Level::High, OutputDrive::Standard),
    })
    .unwrap();

    let _display_kept_off_the_bus = Output::new(pins.display_chip_select, Level::High, OutputDrive::Standard);
    let mut spi_config = spim::Config::default();
    spi_config.frequency = spim::Frequency::M8;
    let spi_bus = Spim::new(
        spi_peripheral,
        Interrupts,
        pins.spi_clock,
        pins.spi_data_in,
        pins.spi_data_out,
        spi_config,
    );
    let flash_chip_select = Output::new(pins.flash_chip_select, Level::High, OutputDrive::Standard);
    let flash_spi = ExclusiveDevice::new_no_delay(spi_bus, flash_chip_select).unwrap();

    let Ok(external_flash) = ExternalFlash::new(flash_spi, delay) else {
        return;
    };

    let internal_flash = Mutex::<NoopRawMutex, _>::new(RefCell::new(PetsWatchdog {
        flash: Nvmc::new(flash_controller),
        watchdog,
    }));
    let external_flash = Mutex::<NoopRawMutex, _>::new(RefCell::new(PetsWatchdog {
        flash: external_flash,
        watchdog,
    }));

    if recovery_requested {
        buzz(&mut vibrator, 200, delay);
        let recovery_copy =
            external_flash.lock(|flash| copy_recovery_into_update_slot(&mut *flash.borrow_mut()));

        let mut state_alignment = AlignedBuffer([0; 4]);
        let mut boot_state = BlockingFirmwareState::new(
            BlockingPartition::new(&internal_flash, BOOT_STATE.start, BOOT_STATE.size),
            &mut state_alignment.0,
        );
        let swap_marked =
            matches!(recovery_copy, Ok(RecoveryCopy::Copied)) && boot_state.mark_updated().is_ok();

        if !swap_marked {
            for _ in 0..3 {
                buzz(&mut vibrator, 60, delay);
                delay.delay_ms(120);
            }
        }
    }

    let config = BootLoaderConfig {
        active: BlockingPartition::new(&internal_flash, FIRMWARE.start, FIRMWARE.size),
        dfu: BlockingPartition::new(&external_flash, FIRMWARE_UPDATE.start, FIRMWARE_UPDATE.size),
        state: BlockingPartition::new(&internal_flash, BOOT_STATE.start, BOOT_STATE.size),
    };
    let _ = BootLoader::<{ PAGE_SIZE as usize }>::try_prepare(config);
}

fn button_held_for(
    duration_in_milliseconds: u32,
    button_input: Peri<'_, P0_13>,
    button_enable: Peri<'_, P0_15>,
    watchdog: &RefCell<WatchdogHandle>,
    delay: &mut Delay,
) -> bool {
    const POLL_INTERVAL_IN_MILLISECONDS: u32 = 10;
    const BUTTON_SETTLE_TIME_IN_MILLISECONDS: u32 = 1;

    let button_pin = Input::new(button_input, Pull::Down);
    let power_pin = Output::new(button_enable, Level::Low, OutputDrive::Standard);
    let Ok(mut button) = Button::new(button_pin, power_pin);
    delay.delay_ms(BUTTON_SETTLE_TIME_IN_MILLISECONDS);

    for _ in 0..duration_in_milliseconds / POLL_INTERVAL_IN_MILLISECONDS {
        if button.is_pressed() != Ok(true) {
            return false;
        }
        watchdog.borrow_mut().pet();
        delay.delay_ms(POLL_INTERVAL_IN_MILLISECONDS);
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

    unsafe { BootLoader::<{ PAGE_SIZE as usize }>.load(FIRMWARE.start) }
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
