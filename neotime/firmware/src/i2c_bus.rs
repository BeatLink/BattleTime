//! The I2C bus the touch panel, the accelerometer and the heart rate sensor share, with its TWIM
//! powered down between transfers.
//!
//! A TWIM that has used EasyDMA keeps drawing about 400 µA until it is disabled and its power
//! register is written 0 then 1 (nRF52832 anomaly 89). That write also clears its settings, so each
//! transfer first puts the pins and speed back and turns the TWIM on, and powers it down afterwards.

use embassy_nrf::twim::{Error, Twim};
use embedded_hal_async::i2c::{ErrorType, I2c, Operation};
use nrf_pac::{self as pac, shared::regs::Psel, twim::regs::Frequency, twim::vals::Enable};

// ============================================================================
// Registers
// ============================================================================

/// TWIM1's power register, at the end of its block and missing from the chip's register map.
const TWIM1_POWER_REGISTER: *mut u32 = 0x4000_4FFC as *mut u32;

// ============================================================================
// The bus
// ============================================================================

/// TWIM1 behind the shared bus, which powers the peripheral down after every transfer.
pub struct SleepingTwim {
    twim: Twim<'static>,
    sda: Psel,
    scl: Psel,
    frequency: Frequency,
}

impl SleepingTwim {
    /// Takes a started TWIM1, remembers its settings and powers it down until the first transfer.
    pub fn new(twim: Twim<'static>) -> SleepingTwim {
        let registers = pac::TWIM1;
        let bus = SleepingTwim {
            twim,
            sda: registers.psel().sda().read(),
            scl: registers.psel().scl().read(),
            frequency: registers.frequency().read(),
        };
        bus.power_down();
        bus
    }

    fn power_up(&self) {
        let registers = pac::TWIM1;
        registers.psel().sda().write_value(self.sda);
        registers.psel().scl().write_value(self.scl);
        registers.frequency().write_value(self.frequency);
        registers
            .enable()
            .write(|enable| enable.set_enable(Enable::Enabled));
    }

    fn power_down(&self) {
        pac::TWIM1
            .enable()
            .write(|enable| enable.set_enable(Enable::Disabled));
        // SAFETY: the register is TWIM1's own, and this is the only place that writes it, so no other state is touched.
        unsafe {
            TWIM1_POWER_REGISTER.write_volatile(0);
            TWIM1_POWER_REGISTER.write_volatile(1);
        }
    }
}

impl ErrorType for SleepingTwim {
    type Error = Error;
}

impl I2c for SleepingTwim {
    async fn transaction(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        self.power_up();
        let result = self.twim.transaction(address, operations).await;
        self.power_down();
        result
    }
}
