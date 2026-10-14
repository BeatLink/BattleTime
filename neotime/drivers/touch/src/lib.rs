//! Driver for the PineTime's Hynitron CST816S touch panel, on I2C address 0x15.
//!
//! The panel pulls its interrupt pin low when a finger lands, moves, lifts or makes a gesture, and
//! keeps pulsing it while the finger stays down. Each pulse is followed by one read of the touch report.

#![no_std]

use embedded_graphics_core::geometry::Point;
use embedded_hal::{digital::OutputPin, i2c::ErrorType as I2cErrorType};
use embedded_hal_async::{delay::DelayNs, digital::Wait, i2c::I2c};

// ============================================================================
// Panel registers
// ============================================================================

pub const I2C_ADDRESS: u8 = 0x15;
pub const PANEL_WIDTH: u16 = 240;
pub const PANEL_HEIGHT: u16 = 240;

const FIRST_REPORT_REGISTER: u8 = 0x01;
const REPORT_LENGTH: usize = 6;
const HIGH_POSITION_BITS: u8 = 0x0f;
const FINGER_COUNT_BITS: u8 = 0x0f;
const FINGER_EVENT_SHIFT: u8 = 6;

const POWER_MODE_REGISTER: u8 = 0xa5;
const DEEP_SLEEP: u8 = 0x03;

const CHIP_ID_REGISTER: u8 = 0xa7;
const PROJECT_ID_REGISTER: u8 = 0xa8;
const FIRMWARE_VERSION_REGISTER: u8 = 0xa9;

const MOTION_MASK_REGISTER: u8 = 0xec;
const REPEAT_LEFT_RIGHT_SLIDES: u8 = 0b0000_0100;
const REPORT_DOUBLE_TAPS: u8 = 0b0000_0001;

const INTERRUPT_CONTROL_REGISTER: u8 = 0xfa;
const INTERRUPT_WHILE_TOUCHING: u8 = 0b0100_0000;
const INTERRUPT_ON_CHANGE: u8 = 0b0010_0000;
const INTERRUPT_ON_GESTURE: u8 = 0b0001_0000;

const AUTO_RESET_REGISTER: u8 = 0xfb;
const AUTO_RESET_OFF: u8 = 0x00;

const RESET_HOLD_IN_MILLISECONDS: u32 = 5;
const RESET_SETTLE_IN_MILLISECONDS: u32 = 50;

// ============================================================================
// Touch reports
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    None,
    SlideDown,
    SlideUp,
    SlideLeft,
    SlideRight,
    Tap,
    DoubleTap,
    LongPress,
}

impl Gesture {
    fn from_code(code: u8) -> Option<Gesture> {
        match code {
            0x00 => Some(Gesture::None),
            0x01 => Some(Gesture::SlideDown),
            0x02 => Some(Gesture::SlideUp),
            0x03 => Some(Gesture::SlideLeft),
            0x04 => Some(Gesture::SlideRight),
            0x05 => Some(Gesture::Tap),
            0x0b => Some(Gesture::DoubleTap),
            0x0c => Some(Gesture::LongPress),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FingerEvent {
    PressedDown,
    LiftedUp,
    StillTouching,
    NoEvent,
}

impl FingerEvent {
    fn from_code(code: u8) -> FingerEvent {
        match code {
            0 => FingerEvent::PressedDown,
            1 => FingerEvent::LiftedUp,
            2 => FingerEvent::StillTouching,
            _ => FingerEvent::NoEvent,
        }
    }
}

/// A place on the panel, in the same columns and rows as the display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchPosition {
    pub column: u16,
    pub row: u16,
}

impl From<TouchPosition> for Point {
    fn from(position: TouchPosition) -> Point {
        Point::new(position.column as i32, position.row as i32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchReport {
    pub position: TouchPosition,
    pub gesture: Gesture,
    pub finger_event: FingerEvent,
    pub is_touching: bool,
}

impl TouchReport {
    /// Decodes registers 0x01 to 0x06, or gives None for a garbled read.
    fn from_registers(registers: &[u8; REPORT_LENGTH]) -> Option<TouchReport> {
        let [gesture_code, finger_count, event_and_column_high, column_low, row_high, row_low] = *registers;

        let column = u16::from(event_and_column_high & HIGH_POSITION_BITS) << 8 | u16::from(column_low);
        let row = u16::from(row_high & HIGH_POSITION_BITS) << 8 | u16::from(row_low);
        if column >= PANEL_WIDTH || row >= PANEL_HEIGHT {
            return None;
        }

        Some(TouchReport {
            position: TouchPosition { column, row },
            gesture: Gesture::from_code(gesture_code)?,
            finger_event: FingerEvent::from_code(event_and_column_high >> FINGER_EVENT_SHIFT),
            is_touching: finger_count & FINGER_COUNT_BITS > 0,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChipInfo {
    pub chip_id: u8,
    pub project_id: u8,
    pub firmware_version: u8,
}

// ============================================================================
// Settings and errors
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Double taps make the panel hold back every single tap while it waits for a second one.
    pub report_double_taps: bool,
}

impl Settings {
    pub const DEFAULT: Settings = Settings {
        report_double_taps: false,
    };
}

impl Default for Settings {
    fn default() -> Settings {
        Settings::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error<BusError, InterruptPinError, ResetPinError> {
    Bus(BusError),
    InterruptPin(InterruptPinError),
    ResetPin(ResetPinError),
}

pub type PanelError<I2cBus, InterruptPin, ResetPin> = Error<
    <I2cBus as I2cErrorType>::Error,
    <InterruptPin as embedded_hal::digital::ErrorType>::Error,
    <ResetPin as embedded_hal::digital::ErrorType>::Error,
>;

// ============================================================================
// Touch panel
// ============================================================================

pub struct TouchPanel<I2cBus, InterruptPin, ResetPin> {
    i2c_bus: I2cBus,
    interrupt_pin: InterruptPin,
    reset_pin: ResetPin,
    settings: Settings,
}

pub struct TouchPanelParts<I2cBus, InterruptPin, ResetPin> {
    pub i2c_bus: I2cBus,
    pub interrupt_pin: InterruptPin,
    pub reset_pin: ResetPin,
}

impl<I2cBus, InterruptPin, ResetPin> TouchPanel<I2cBus, InterruptPin, ResetPin>
where
    I2cBus: I2c,
    InterruptPin: Wait,
    ResetPin: OutputPin,
{
    /// Takes the panel's bus and pins without talking to it; call `start` next.
    pub fn new(i2c_bus: I2cBus, interrupt_pin: InterruptPin, reset_pin: ResetPin) -> Self {
        TouchPanel {
            i2c_bus,
            interrupt_pin,
            reset_pin,
            settings: Settings::DEFAULT,
        }
    }

    /// Resets the panel and sets it to interrupt on every touch, move, lift and gesture.
    pub async fn start(
        &mut self,
        settings: Settings,
        delay: &mut impl DelayNs,
    ) -> Result<(), PanelError<I2cBus, InterruptPin, ResetPin>> {
        self.settings = settings;
        self.reset(delay).await?;

        let mut motion_mask = REPEAT_LEFT_RIGHT_SLIDES;
        if settings.report_double_taps {
            motion_mask |= REPORT_DOUBLE_TAPS;
        }
        let interrupt_control = INTERRUPT_WHILE_TOUCHING | INTERRUPT_ON_CHANGE | INTERRUPT_ON_GESTURE;

        self.write_register(MOTION_MASK_REGISTER, motion_mask).await?;
        self.write_register(INTERRUPT_CONTROL_REGISTER, interrupt_control)
            .await?;
        self.write_register(AUTO_RESET_REGISTER, AUTO_RESET_OFF).await?;

        Ok(())
    }

    /// Waits for the next interrupt pulse and reads its report, skipping garbled reads.
    pub async fn wait_for_report(
        &mut self,
    ) -> Result<TouchReport, PanelError<I2cBus, InterruptPin, ResetPin>> {
        loop {
            self.interrupt_pin
                .wait_for_falling_edge()
                .await
                .map_err(Error::InterruptPin)?;

            if let Some(report) = self.read_report().await.map_err(Error::Bus)? {
                return Ok(report);
            }
        }
    }

    /// Reads the latest report now, or gives None when its numbers make no sense.
    pub async fn read_report(&mut self) -> Result<Option<TouchReport>, I2cBus::Error> {
        let mut registers = [0; REPORT_LENGTH];
        self.i2c_bus
            .write_read(I2C_ADDRESS, &[FIRST_REPORT_REGISTER], &mut registers)
            .await?;

        Ok(TouchReport::from_registers(&registers))
    }

    /// The panel answers only while awake, so read this right after `start` or a touch.
    pub async fn read_chip_info(&mut self) -> Result<ChipInfo, I2cBus::Error> {
        Ok(ChipInfo {
            chip_id: self.read_register(CHIP_ID_REGISTER).await?,
            project_id: self.read_register(PROJECT_ID_REGISTER).await?,
            firmware_version: self.read_register(FIRMWARE_VERSION_REGISTER).await?,
        })
    }

    /// Puts the panel into deep sleep, where it ignores touches until `wake`.
    pub async fn sleep(
        &mut self,
        delay: &mut impl DelayNs,
    ) -> Result<(), PanelError<I2cBus, InterruptPin, ResetPin>> {
        self.reset(delay).await?;

        if self
            .write_register(POWER_MODE_REGISTER, DEEP_SLEEP)
            .await
            .is_err()
        {
            self.reset_pin.set_low().map_err(Error::ResetPin)?;
        }

        Ok(())
    }

    /// Keeps the panel listening only for a double tap, which then pulses the interrupt pin.
    pub async fn sleep_until_double_tap(
        &mut self,
        delay: &mut impl DelayNs,
    ) -> Result<(), PanelError<I2cBus, InterruptPin, ResetPin>> {
        self.reset(delay).await?;

        self.write_register(MOTION_MASK_REGISTER, REPORT_DOUBLE_TAPS)
            .await
    }

    /// Wakes the panel from either kind of sleep, with the settings `start` was last given.
    pub async fn wake(
        &mut self,
        delay: &mut impl DelayNs,
    ) -> Result<(), PanelError<I2cBus, InterruptPin, ResetPin>> {
        self.start(self.settings, delay).await
    }

    pub fn release(self) -> TouchPanelParts<I2cBus, InterruptPin, ResetPin> {
        TouchPanelParts {
            i2c_bus: self.i2c_bus,
            interrupt_pin: self.interrupt_pin,
            reset_pin: self.reset_pin,
        }
    }

    /// A reset also wakes the panel's I2C, which stops answering while the panel is idle.
    async fn reset(
        &mut self,
        delay: &mut impl DelayNs,
    ) -> Result<(), PanelError<I2cBus, InterruptPin, ResetPin>> {
        self.reset_pin.set_low().map_err(Error::ResetPin)?;
        delay.delay_ms(RESET_HOLD_IN_MILLISECONDS).await;
        self.reset_pin.set_high().map_err(Error::ResetPin)?;
        delay.delay_ms(RESET_SETTLE_IN_MILLISECONDS).await;

        Ok(())
    }

    async fn read_register(&mut self, register: u8) -> Result<u8, I2cBus::Error> {
        let mut value = [0];
        self.i2c_bus
            .write_read(I2C_ADDRESS, &[register], &mut value)
            .await?;

        Ok(value[0])
    }

    async fn write_register(
        &mut self,
        register: u8,
        value: u8,
    ) -> Result<(), PanelError<I2cBus, InterruptPin, ResetPin>> {
        self.i2c_bus
            .write(I2C_ADDRESS, &[register, value])
            .await
            .map_err(Error::Bus)
    }
}

#[cfg(test)]
mod tests;
