//! Driver for the PineTime's Tianyi HRS3300 heart rate sensor, on I2C address 0x44.
//!
//! The chip shines its green LED into the skin and measures what comes back, the pulse channel,
//! alongside an ambient light channel. `ppg` turns the pulse channel into beats per minute, and
//! `monitor::Monitor` adds the sensor's status and spots a watch taken off the wrist. `schedule`
//! decides between measuring for an app, measuring in the background and flashing the LED as a beacon.

#![no_std]

use embedded_hal::i2c::ErrorType as I2cErrorType;
use embedded_hal_async::i2c::I2c;

pub mod monitor;
pub mod ppg;
pub mod schedule;

// ============================================================================
// Chip registers
// ============================================================================

pub const I2C_ADDRESS: u8 = 0x44;

const ID_REGISTER: u8 = 0x00;
const HRS3300_ID: u8 = 0x21;

const ENABLE_REGISTER: u8 = 0x01;
const MEASURING_ON: u8 = 0x80;
const WAIT_12_5_MILLISECONDS: u8 = 0x60;
const WAIT_TIME_SHIFT: u8 = 4;
/// The drive level's high bit lives in the enable register, its low bit in the LED drive register.
const DRIVE_HIGH_BIT_IN_ENABLE: u8 = 0x08;
const DRIVE_LOW_BIT_IN_LED_DRIVE: u8 = 0x40;

const LED_DRIVE_REGISTER: u8 = 0x0c;
/// The low bits the LED drive register needs, as wasp-os and InfiniTime both set them.
const LED_DRIVE_RESERVED_BITS: u8 = 0x2e;
const LED_DRIVE_20_MILLIAMPS: u8 = LED_DRIVE_RESERVED_BITS | DRIVE_LOW_BIT_IN_LED_DRIVE;
const LED_OFF: u8 = 0x00;

const RESOLUTION_REGISTER: u8 = 0x16;
const BOTH_CHANNELS_16_BITS: u8 = 0x88;

const GAIN_REGISTER: u8 = 0x17;
const GAIN_ONE: u8 = 0x00;

/// The six data registers sit between these two, so one read takes them all.
const FIRST_DATA_REGISTER: u8 = 0x08;
const DATA_LENGTH: usize = 8;

// ============================================================================
// Samples and errors
// ============================================================================

/// One conversion of both channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sample {
    pub pulse: u32,
    pub ambient_light: u32,
}

impl Sample {
    /// Unpacks the channels from registers 0x08 to 0x0f, as the datasheet spreads their bits.
    fn from_registers(registers: &[u8; DATA_LENGTH]) -> Sample {
        let [ambient_middle, pulse_middle, pulse_high, _, _, ambient_high, ambient_low, pulse_low] =
            *registers;

        let pulse = u32::from(pulse_middle) << 8
            | u32::from(pulse_high & 0x0f) << 4
            | u32::from(pulse_low & 0x0f)
            | u32::from(pulse_low & 0x30) << 12;
        let ambient_light = u32::from(ambient_high & 0x3f) << 11
            | u32::from(ambient_middle) << 3
            | u32::from(ambient_low & 0x07);
        Sample { pulse, ambient_light }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error<BusError> {
    Bus(BusError),
    /// The chip at the address gave this id instead of the HRS3300's.
    WrongChip(u8),
}

// ============================================================================
// The driver
// ============================================================================

/// Each pair is a register and the value written to it, in order.
const START_WRITES: [[u8; 2]; 5] = [
    [ENABLE_REGISTER, WAIT_12_5_MILLISECONDS],
    [LED_DRIVE_REGISTER, LED_DRIVE_20_MILLIAMPS],
    [RESOLUTION_REGISTER, BOTH_CHANNELS_16_BITS],
    [GAIN_REGISTER, GAIN_ONE],
    [ENABLE_REGISTER, WAIT_12_5_MILLISECONDS | MEASURING_ON],
];
const STOP_WRITES: [[u8; 2]; 2] = [
    [ENABLE_REGISTER, WAIT_12_5_MILLISECONDS],
    [LED_DRIVE_REGISTER, LED_OFF],
];

/// What `perform` asks of the chip, so a caller can reach it from one place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Checks the chip, then turns the LED on and starts a conversion every 12.5 ms.
    Start,
    /// Stops measuring and turns the LED off.
    Stop,
    /// Reads the latest conversion of both channels.
    Read,
    /// Flashes the LED at drive level 0 (12.5 mA) to 3 (40 mA), waiting between flashes by wait time 0
    /// (800 ms) to 7 (none), as wasp-os's beacon does.
    Flash { drive: u8, wait: u8 },
}

pub struct HeartRateSensor<Bus> {
    bus: Bus,
}

impl<Bus: I2c> HeartRateSensor<Bus> {
    pub fn new(bus: Bus) -> HeartRateSensor<Bus> {
        HeartRateSensor { bus }
    }

    pub async fn start(&mut self) -> Result<(), Error<<Bus as I2cErrorType>::Error>> {
        self.perform(Command::Start).await.map(drop)
    }

    pub async fn stop(&mut self) -> Result<(), Error<<Bus as I2cErrorType>::Error>> {
        self.perform(Command::Stop).await.map(drop)
    }

    pub async fn read(&mut self) -> Result<Sample, Error<<Bus as I2cErrorType>::Error>> {
        self.perform(Command::Read).await
    }

    /// Carries out the command, and gives the sample a `Read` took, or an empty one.
    pub async fn perform(&mut self, command: Command) -> Result<Sample, Error<<Bus as I2cErrorType>::Error>> {
        let mut registers = [0; DATA_LENGTH];
        let flash_writes;
        let (first_register_read, length_read, writes): (u8, usize, &[[u8; 2]]) = match command {
            Command::Start => (ID_REGISTER, 1, &START_WRITES),
            Command::Stop => (ID_REGISTER, 0, &STOP_WRITES),
            Command::Read => (FIRST_DATA_REGISTER, DATA_LENGTH, &[]),
            Command::Flash { drive, wait } => {
                let high_drive = if drive & 0x02 != 0 {
                    DRIVE_HIGH_BIT_IN_ENABLE
                } else {
                    0
                };
                let low_drive = if drive & 0x01 != 0 {
                    DRIVE_LOW_BIT_IN_LED_DRIVE
                } else {
                    0
                };
                flash_writes = [
                    [
                        ENABLE_REGISTER,
                        MEASURING_ON | (wait & 0x07) << WAIT_TIME_SHIFT | high_drive,
                    ],
                    [LED_DRIVE_REGISTER, LED_DRIVE_RESERVED_BITS | low_drive],
                ];
                (ID_REGISTER, 0, &flash_writes)
            }
        };

        if length_read > 0 {
            self.bus
                .write_read(I2C_ADDRESS, &[first_register_read], &mut registers[..length_read])
                .await
                .map_err(Error::Bus)?;
        }
        if command == Command::Start && registers[0] != HRS3300_ID {
            return Err(Error::WrongChip(registers[0]));
        }
        for write in writes {
            self.bus.write(I2C_ADDRESS, write).await.map_err(Error::Bus)?;
        }

        Ok(match command {
            Command::Read => Sample::from_registers(&registers),
            _ => Sample::default(),
        })
    }
}

#[cfg(test)]
mod tests;
