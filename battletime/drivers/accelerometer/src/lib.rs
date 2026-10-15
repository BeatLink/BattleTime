//! Driver for the PineTime's Bosch BMA421 or BMA425 accelerometer, on I2C address 0x18.
//!
//! The chip counts steps by itself once Bosch's configuration file for it has been uploaded, so the
//! watch only reads the count. The files are not built into the firmware: the shared library
//! carries them, and `start` takes them from the caller. The `configuration-files` feature gives
//! them to the compiler, which puts them in the library. Acceleration is read on demand, turned to the watch's own axes, and
//! `shake::ShakeDetector` finds shakes in those readings, while `wake_gestures::WakeGestureDetector`
//! finds a raised or lowered wrist and a shake strong enough to wake the screen.

#![no_std]

use embedded_hal::i2c::ErrorType as I2cErrorType;
use embedded_hal_async::{delay::DelayNs, i2c::I2c};

pub mod shake;
pub mod wake_gestures;

// ============================================================================
// Chip registers
// ============================================================================

pub const I2C_ADDRESS: u8 = 0x18;

const CHIP_ID_REGISTER: u8 = 0x00;
const BMA421_CHIP_ID: u8 = 0x11;
const BMA425_CHIP_ID: u8 = 0x13;

const ACCELERATION_REGISTER: u8 = 0x12;
const ACCELERATION_LENGTH: usize = 6;
const STEP_COUNT_REGISTER: u8 = 0x1e;
const STEP_COUNT_LENGTH: usize = 4;

const INTERNAL_STATUS_REGISTER: u8 = 0x2a;
const INTERNAL_STATUS_MESSAGE_BITS: u8 = 0x0f;
const INTERNAL_STATUS_INITIALIZED: u8 = 0x01;

const ACCELERATION_CONFIG_REGISTER: u8 = 0x40;
const SAMPLE_RATE_100_HERTZ: u8 = 0x08;
const AVERAGE_OF_4_SAMPLES: u8 = 0x20;

const ACCELERATION_RANGE_REGISTER: u8 = 0x41;

const CONFIGURATION_LOAD_REGISTER: u8 = 0x59;
const CONFIGURATION_LOAD_STOPPED: u8 = 0x00;
const CONFIGURATION_LOAD_STARTED: u8 = 0x01;
const UPLOAD_ADDRESS_LOW_REGISTER: u8 = 0x5b;
const UPLOAD_ADDRESS_HIGH_REGISTER: u8 = 0x5c;
const UPLOAD_ADDRESS_LOW_BITS: u16 = 0x0f;
const UPLOAD_ADDRESS_HIGH_SHIFT: u16 = 4;
const UPLOAD_CHUNK_LENGTH: usize = 16;

const FEATURE_CONFIGURATION_REGISTER: u8 = 0x5e;
const FEATURE_CONFIGURATION_LENGTH: usize = 70;
const STEP_COUNTER_CONTROL_OFFSET: usize = 0x3b;
const STEP_COUNTER_ON: u8 = 0x10;
const STEP_COUNT_RESET: u8 = 0x04;

const INTERFACE_WATCHDOG_REGISTER: u8 = 0x70;
const INTERFACE_WATCHDOG_ON_AFTER_40_MILLISECONDS: u8 = 0x06;

const POWER_CONFIG_REGISTER: u8 = 0x7c;
const POWER_SAVING_ON: u8 = 0x03;
const POWER_SAVING_OFF: u8 = 0x02;

const POWER_CONTROL_REGISTER: u8 = 0x7d;
const ACCELEROMETER_ON: u8 = 0x04;

const COMMAND_REGISTER: u8 = 0x7e;
const SOFT_RESET: u8 = 0xb6;

const SOFT_RESET_SETTLE_IN_MILLISECONDS: u32 = 50;
const POWER_SAVING_SWITCH_IN_MICROSECONDS: u32 = 450;
const CONFIGURATION_LOAD_IN_MILLISECONDS: u32 = 150;

// ============================================================================
// Chip models
// ============================================================================

/// Bosch's configuration file for each chip, for the compiler to put in the shared library.
#[cfg(any(test, feature = "configuration-files"))]
pub const BMA421_CONFIGURATION_FILE: &[u8] = include_bytes!("configuration/bma421.bin");
#[cfg(any(test, feature = "configuration-files"))]
pub const BMA425_CONFIGURATION_FILE: &[u8] = include_bytes!("configuration/bma425.bin");

/// The configuration files `start` can upload, each where the caller found it, or none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Configurations<'files> {
    pub bma421: Option<&'files [u8]>,
    pub bma425: Option<&'files [u8]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipModel {
    Bma421,
    Bma425,
}

impl ChipModel {
    /// The ID the chip reports in its chip ID register.
    pub fn chip_id(self) -> u8 {
        match self {
            ChipModel::Bma421 => BMA421_CHIP_ID,
            ChipModel::Bma425 => BMA425_CHIP_ID,
        }
    }

    fn from_chip_id(chip_id: u8) -> Option<ChipModel> {
        match chip_id {
            BMA421_CHIP_ID => Some(ChipModel::Bma421),
            BMA425_CHIP_ID => Some(ChipModel::Bma425),
            _ => None,
        }
    }

    fn configuration_file(self, configurations: Configurations<'_>) -> Option<&[u8]> {
        match self {
            ChipModel::Bma421 => configurations.bma421,
            ChipModel::Bma425 => configurations.bma425,
        }
    }
}

// ============================================================================
// Readings and orientation
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Acceleration {
    pub x_in_milli_g: i16,
    pub y_in_milli_g: i16,
    pub z_in_milli_g: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipAxis {
    X,
    Y,
    Z,
}

/// Which of the chip's axes feeds one of the watch's axes, and whether it points the other way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AxisSource {
    pub chip_axis: ChipAxis,
    pub is_reversed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Orientation {
    pub watch_x: AxisSource,
    pub watch_y: AxisSource,
    pub watch_z: AxisSource,
}

impl Orientation {
    /// The chip's own axes, unchanged.
    pub const CHIP: Orientation = Orientation {
        watch_x: AxisSource {
            chip_axis: ChipAxis::X,
            is_reversed: false,
        },
        watch_y: AxisSource {
            chip_axis: ChipAxis::Y,
            is_reversed: false,
        },
        watch_z: AxisSource {
            chip_axis: ChipAxis::Z,
            is_reversed: false,
        },
    };

    /// How the chip sits on the PineTime's board, the same as wasp-os uses.
    pub const PINETIME: Orientation = Orientation {
        watch_x: AxisSource {
            chip_axis: ChipAxis::Y,
            is_reversed: false,
        },
        watch_y: AxisSource {
            chip_axis: ChipAxis::X,
            is_reversed: true,
        },
        watch_z: AxisSource {
            chip_axis: ChipAxis::Z,
            is_reversed: false,
        },
    };
}

/// The three raw 12-bit readings, in the chip's own axes.
struct ChipReading {
    x: i16,
    y: i16,
    z: i16,
}

impl ChipReading {
    fn from_registers(registers: &[u8; ACCELERATION_LENGTH]) -> ChipReading {
        let [x_low, x_high, y_low, y_high, z_low, z_high] = *registers;

        let twelve_bits = |low: u8, high: u8| i16::from_le_bytes([low, high]) / 16;
        ChipReading {
            x: twelve_bits(x_low, x_high),
            y: twelve_bits(y_low, y_high),
            z: twelve_bits(z_low, z_high),
        }
    }

    fn along(&self, source: AxisSource) -> i16 {
        let reading = match source.chip_axis {
            ChipAxis::X => self.x,
            ChipAxis::Y => self.y,
            ChipAxis::Z => self.z,
        };

        if source.is_reversed {
            -reading
        } else {
            reading
        }
    }
}

// ============================================================================
// Settings and errors
// ============================================================================

/// The largest acceleration the chip can report; a smaller range reads finer steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    TwoG,
    FourG,
    EightG,
    SixteenG,
}

impl Range {
    fn register_value(self) -> u8 {
        match self {
            Range::TwoG => 0x00,
            Range::FourG => 0x01,
            Range::EightG => 0x02,
            Range::SixteenG => 0x03,
        }
    }

    fn full_scale_in_g(self) -> i32 {
        match self {
            Range::TwoG => 2,
            Range::FourG => 4,
            Range::EightG => 8,
            Range::SixteenG => 16,
        }
    }

    fn to_milli_g(self, reading: i16) -> i16 {
        const HALF_OF_TWELVE_BIT_SPAN: i32 = 2048;

        (i32::from(reading) * 1000 * self.full_scale_in_g() / HALF_OF_TWELVE_BIT_SPAN) as i16
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub range: Range,
    pub orientation: Orientation,
}

impl Settings {
    pub const DEFAULT: Settings = Settings {
        range: Range::TwoG,
        orientation: Orientation::PINETIME,
    };
}

impl Default for Settings {
    fn default() -> Settings {
        Settings::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error<BusError> {
    Bus(BusError),
    UnknownChip {
        chip_id: u8,
    },
    /// The caller has no configuration file for this chip, such as a firmware built without the shared library.
    NoConfiguration {
        chip_model: ChipModel,
    },
    ConfigurationRejected {
        internal_status: u8,
    },
}

pub type AccelerometerError<I2cBus> = Error<<I2cBus as I2cErrorType>::Error>;

// ============================================================================
// Accelerometer
// ============================================================================

pub struct Accelerometer<I2cBus> {
    i2c_bus: I2cBus,
    settings: Settings,
}

impl<I2cBus> Accelerometer<I2cBus>
where
    I2cBus: I2c,
{
    /// Takes the chip's bus without talking to it; call `start` next.
    pub fn new(i2c_bus: I2cBus) -> Self {
        Accelerometer {
            i2c_bus,
            settings: Settings::DEFAULT,
        }
    }

    /// Resets the chip, uploads its configuration file from `configurations` and starts it measuring and counting steps.
    pub async fn start(
        &mut self,
        settings: Settings,
        delay: &mut impl DelayNs,
        configurations: Configurations<'_>,
    ) -> Result<ChipModel, AccelerometerError<I2cBus>> {
        self.settings = settings;

        self.write_register(COMMAND_REGISTER, SOFT_RESET).await?;
        delay.delay_ms(SOFT_RESET_SETTLE_IN_MILLISECONDS).await;

        let chip_id = self.read_register(CHIP_ID_REGISTER).await?;
        let chip_model = ChipModel::from_chip_id(chip_id).ok_or(Error::UnknownChip { chip_id })?;
        let configuration_file = chip_model
            .configuration_file(configurations)
            .ok_or(Error::NoConfiguration { chip_model })?;

        self.write_register(
            INTERFACE_WATCHDOG_REGISTER,
            INTERFACE_WATCHDOG_ON_AFTER_40_MILLISECONDS,
        )
        .await?;
        self.upload_configuration_file(configuration_file, delay).await?;

        self.write_register(POWER_CONTROL_REGISTER, ACCELEROMETER_ON)
            .await?;
        self.write_register(
            ACCELERATION_CONFIG_REGISTER,
            SAMPLE_RATE_100_HERTZ | AVERAGE_OF_4_SAMPLES,
        )
        .await?;
        self.write_register(ACCELERATION_RANGE_REGISTER, settings.range.register_value())
            .await?;
        self.edit_feature_configuration(delay, |configuration| {
            configuration[STEP_COUNTER_CONTROL_OFFSET] |= STEP_COUNTER_ON;
        })
        .await
        .map_err(Error::Bus)?;

        Ok(chip_model)
    }

    /// Reads the latest acceleration, turned to the watch's axes by the settings' orientation.
    pub async fn read_acceleration(&mut self) -> Result<Acceleration, I2cBus::Error> {
        let mut registers = [0; ACCELERATION_LENGTH];
        self.i2c_bus
            .write_read(I2C_ADDRESS, &[ACCELERATION_REGISTER], &mut registers)
            .await?;

        let reading = ChipReading::from_registers(&registers);
        let orientation = self.settings.orientation;
        let range = self.settings.range;
        Ok(Acceleration {
            x_in_milli_g: range.to_milli_g(reading.along(orientation.watch_x)),
            y_in_milli_g: range.to_milli_g(reading.along(orientation.watch_y)),
            z_in_milli_g: range.to_milli_g(reading.along(orientation.watch_z)),
        })
    }

    /// Steps counted since `start` or the last `reset_step_count`.
    pub async fn read_step_count(&mut self) -> Result<u32, I2cBus::Error> {
        let mut registers = [0; STEP_COUNT_LENGTH];
        self.i2c_bus
            .write_read(I2C_ADDRESS, &[STEP_COUNT_REGISTER], &mut registers)
            .await?;

        Ok(u32::from_le_bytes(registers))
    }

    pub async fn reset_step_count(&mut self, delay: &mut impl DelayNs) -> Result<(), I2cBus::Error> {
        self.edit_feature_configuration(delay, |configuration| {
            configuration[STEP_COUNTER_CONTROL_OFFSET] |= STEP_COUNT_RESET;
        })
        .await
    }

    pub fn release(self) -> I2cBus {
        self.i2c_bus
    }

    /// Streams the file into the chip's feature memory, then waits for the chip to accept it.
    async fn upload_configuration_file(
        &mut self,
        configuration_file: &[u8],
        delay: &mut impl DelayNs,
    ) -> Result<(), AccelerometerError<I2cBus>> {
        self.write_register(POWER_CONFIG_REGISTER, POWER_SAVING_OFF)
            .await?;
        delay.delay_us(POWER_SAVING_SWITCH_IN_MICROSECONDS).await;
        self.write_register(CONFIGURATION_LOAD_REGISTER, CONFIGURATION_LOAD_STOPPED)
            .await?;

        for (chunk_number, chunk) in configuration_file.chunks(UPLOAD_CHUNK_LENGTH).enumerate() {
            let word_address = (chunk_number * UPLOAD_CHUNK_LENGTH / 2) as u16;
            let address_low = (word_address & UPLOAD_ADDRESS_LOW_BITS) as u8;
            let address_high = (word_address >> UPLOAD_ADDRESS_HIGH_SHIFT) as u8;
            self.write_register(UPLOAD_ADDRESS_LOW_REGISTER, address_low)
                .await?;
            self.write_register(UPLOAD_ADDRESS_HIGH_REGISTER, address_high)
                .await?;
            self.write_registers(FEATURE_CONFIGURATION_REGISTER, chunk)
                .await
                .map_err(Error::Bus)?;
        }

        self.write_register(CONFIGURATION_LOAD_REGISTER, CONFIGURATION_LOAD_STARTED)
            .await?;
        delay.delay_ms(CONFIGURATION_LOAD_IN_MILLISECONDS).await;

        let internal_status =
            self.read_register(INTERNAL_STATUS_REGISTER).await? & INTERNAL_STATUS_MESSAGE_BITS;
        if internal_status != INTERNAL_STATUS_INITIALIZED {
            return Err(Error::ConfigurationRejected { internal_status });
        }

        Ok(())
    }

    /// The chip takes feature memory writes only with power saving off, so it is paused around the edit.
    async fn edit_feature_configuration(
        &mut self,
        delay: &mut impl DelayNs,
        edit: impl FnOnce(&mut [u8; FEATURE_CONFIGURATION_LENGTH]),
    ) -> Result<(), I2cBus::Error> {
        self.write_registers(POWER_CONFIG_REGISTER, &[POWER_SAVING_OFF])
            .await?;
        delay.delay_us(POWER_SAVING_SWITCH_IN_MICROSECONDS).await;

        let mut configuration = [0; FEATURE_CONFIGURATION_LENGTH];
        self.i2c_bus
            .write_read(I2C_ADDRESS, &[FEATURE_CONFIGURATION_REGISTER], &mut configuration)
            .await?;
        edit(&mut configuration);
        self.write_registers(FEATURE_CONFIGURATION_REGISTER, &configuration)
            .await?;

        self.write_registers(POWER_CONFIG_REGISTER, &[POWER_SAVING_ON])
            .await?;
        delay.delay_us(POWER_SAVING_SWITCH_IN_MICROSECONDS).await;

        Ok(())
    }

    async fn read_register(&mut self, register: u8) -> Result<u8, AccelerometerError<I2cBus>> {
        let mut value = [0];
        self.i2c_bus
            .write_read(I2C_ADDRESS, &[register], &mut value)
            .await
            .map_err(Error::Bus)?;

        Ok(value[0])
    }

    async fn write_register(&mut self, register: u8, value: u8) -> Result<(), AccelerometerError<I2cBus>> {
        self.write_registers(register, &[value]).await.map_err(Error::Bus)
    }

    /// Writes values to consecutive registers, or a run of bytes into one streaming register.
    async fn write_registers(&mut self, first_register: u8, values: &[u8]) -> Result<(), I2cBus::Error> {
        const LONGEST_WRITE: usize = FEATURE_CONFIGURATION_LENGTH;

        let mut message = [0; 1 + LONGEST_WRITE];
        message[0] = first_register;
        message[1..=values.len()].copy_from_slice(values);
        self.i2c_bus.write(I2C_ADDRESS, &message[..=values.len()]).await
    }
}

#[cfg(test)]
mod tests;
