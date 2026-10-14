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

/// The chip ID, then the project ID and the firmware version in the two registers after it.
const CHIP_ID_REGISTER: u8 = 0xa7;

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

// ============================================================================
// One finger from landing to lifting
// ============================================================================

/// The farthest a finger moves between two reports; a report farther off is noise, unless the next one agrees with it.
pub const LARGEST_JUMP_IN_PIXELS: i32 = 60;

/// The panel can send one more report at the place a finger lifted from, this soon after the lift.
pub const LIFT_ECHO_MILLISECONDS: u64 = 30;

/// What a report means for the finger on the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FingerStep {
    Lands,
    Moves,
    Lifts,
    Nothing,
}

/// Whether a finger is on the panel and where it was last, so a landing reported while one is already down counts as a move.
///
/// While a finger slides left or right the panel repeats the slide, and can report the finger landing again with each repeat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Finger {
    is_down: bool,
    last_position: Point,
    /// A far-off position held back, taken as real only if the next report lands near it.
    doubtful_position: Option<Point>,
    /// Where and when the last reported lift was, so the panel's late report of it is not taken as a new finger.
    last_lift: Option<(Point, u64)>,
}

impl Finger {
    /// Takes a report and when it came, and says what it means for the finger.
    pub fn step(&mut self, report: &TouchReport, now_in_milliseconds: u64) -> FingerStep {
        let position = Point::from(report.position);
        match report.finger_event {
            FingerEvent::PressedDown | FingerEvent::StillTouching
                if !self.is_down && self.echoes_last_lift(position, now_in_milliseconds) =>
            {
                FingerStep::Nothing
            }
            // A move whose landing was lost lands the finger where it is.
            FingerEvent::PressedDown | FingerEvent::StillTouching if !self.is_down => {
                *self = Finger {
                    is_down: true,
                    last_position: position,
                    doubtful_position: None,
                    last_lift: None,
                };
                FingerStep::Lands
            }
            FingerEvent::PressedDown | FingerEvent::StillTouching => match self.is_believable(position) {
                true => FingerStep::Moves,
                false => FingerStep::Nothing,
            },
            FingerEvent::LiftedUp if self.is_down => {
                self.is_down = false;
                self.last_lift = Some((position, now_in_milliseconds));
                FingerStep::Lifts
            }
            // A lift `lift` already ended, or the same lift reported twice.
            FingerEvent::LiftedUp => FingerStep::Nothing,
            FingerEvent::NoEvent => {
                self.is_down &= report.is_touching;
                FingerStep::Nothing
            }
        }
    }

    pub fn is_down(&self) -> bool {
        self.is_down
    }

    /// Ends a touch whose lift the panel never reported, giving where the finger was last.
    pub fn lift(&mut self) -> Option<Point> {
        let was_down = self.is_down;
        self.is_down = false;
        was_down.then_some(self.last_position)
    }

    /// Whether a report comes so soon after the last lift, and so near it, that it is the panel repeating the lift.
    fn echoes_last_lift(&self, position: Point, now_in_milliseconds: u64) -> bool {
        self.last_lift.is_some_and(|(lifted_at, lift_time)| {
            let gap = position - lifted_at;
            now_in_milliseconds.saturating_sub(lift_time) <= LIFT_ECHO_MILLISECONDS
                && gap.x.abs().max(gap.y.abs()) <= DRAG_THRESHOLD_IN_PIXELS
        })
    }

    /// Whether a position is near the last one, or near a far-off one the report before gave too.
    fn is_believable(&mut self, position: Point) -> bool {
        let is_near = |other: Point| {
            let gap = position - other;
            gap.x.abs().max(gap.y.abs()) <= LARGEST_JUMP_IN_PIXELS
        };
        if is_near(self.last_position) || self.doubtful_position.is_some_and(is_near) {
            self.last_position = position;
            self.doubtful_position = None;
            return true;
        }
        self.doubtful_position = Some(position);
        false
    }
}

// ============================================================================
// Gestures read from the finger's path
// ============================================================================

/// How far a finger moves before a touch counts as a drag, not a tap or a long press.
pub const DRAG_THRESHOLD_IN_PIXELS: i32 = 10;

/// How far a finger moves along one direction before the touch is a swipe, while it is still down.
pub const SWIPE_DISTANCE_IN_PIXELS: i32 = 30;

/// How far a flick moves before its lift can count as a swipe.
pub const FLICK_DISTANCE_IN_PIXELS: i32 = 20;

/// How fast a finger must be moving the flick's way as it lifts.
pub const FLICK_SPEED_IN_PIXELS_PER_SECOND: i32 = 400;

/// The finger's speed is measured over about this long before it lifts.
pub const SPEED_WINDOW_MILLISECONDS: u64 = 80;

/// The fastest a lift's speed is given, so a short list does not shoot past its end.
pub const FASTEST_FLING_IN_PIXELS_PER_SECOND: i32 = 2500;

/// How long a finger stays still before the touch is a long press.
pub const LONG_PRESS_MILLISECONDS: u64 = 500;

/// A touch shorter than this is a bounce or a brush of a sleeve, not a tap.
pub const SHORTEST_TAP_MILLISECONDS: u64 = 30;

/// How many recent reports the finger's speed is read from.
const RECENT_REPORTS: usize = 8;

/// Finds a touch's one tap, long press or swipe from where and when the finger lands, moves and lifts.
///
/// The panel's own gestures come late and only for long straight strokes, so they are not used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GestureFinder {
    landing: Option<(Point, u64)>,
    recent: [(Point, u64); RECENT_REPORTS],
    reports_seen: usize,
    speed: Point,
    dragged: bool,
    found: bool,
}

impl GestureFinder {
    /// Takes one finger step, where the finger was and when, and gives the touch's gesture once.
    pub fn step(&mut self, step: FingerStep, position: Point, now_in_milliseconds: u64) -> Gesture {
        let lifted = match (step, self.landing) {
            // A move whose landing was lost starts the touch where it is.
            (FingerStep::Lands, _) | (FingerStep::Moves, None) => {
                *self = GestureFinder {
                    landing: Some((position, now_in_milliseconds)),
                    ..GestureFinder::default()
                };
                self.remember(position, now_in_milliseconds);
                return Gesture::None;
            }
            (FingerStep::Nothing, _) | (FingerStep::Lifts, None) => return Gesture::None,
            (FingerStep::Moves, Some(_)) => false,
            (FingerStep::Lifts, Some(_)) => true,
        };
        let Some((start, landed_at)) = self.landing else {
            return Gesture::None;
        };
        self.speed = self.speed_at(position, now_in_milliseconds);
        self.remember(position, now_in_milliseconds);
        let moved = position - start;
        let held_for = now_in_milliseconds.saturating_sub(landed_at);
        self.dragged |= moved.x.abs().max(moved.y.abs()) > DRAG_THRESHOLD_IN_PIXELS;
        if lifted {
            self.landing = None;
        }
        if self.found {
            return Gesture::None;
        }

        let gesture = match (lifted, self.dragged) {
            (false, true) => swipe_along(moved, SWIPE_DISTANCE_IN_PIXELS),
            (false, false) if held_for >= LONG_PRESS_MILLISECONDS => Gesture::LongPress,
            (false, false) => Gesture::None,
            (true, true) => match swipe_along(moved, FLICK_DISTANCE_IN_PIXELS) {
                flick if is_fast_along(self.speed, flick) => flick,
                _ => swipe_along(moved, SWIPE_DISTANCE_IN_PIXELS),
            },
            (true, false) if held_for >= LONG_PRESS_MILLISECONDS => Gesture::LongPress,
            (true, false) if held_for < SHORTEST_TAP_MILLISECONDS => Gesture::None,
            (true, false) => Gesture::Tap,
        };
        self.found = gesture != Gesture::None;
        gesture
    }

    /// How fast the finger was moving, in pixels a second, at its last report, capped at the fastest fling.
    pub fn speed(&self) -> Point {
        let fastest = self.speed.x.abs().max(self.speed.y.abs());
        if fastest <= FASTEST_FLING_IN_PIXELS_PER_SECOND {
            return self.speed;
        }
        self.speed * FASTEST_FLING_IN_PIXELS_PER_SECOND / fastest
    }

    fn remember(&mut self, position: Point, now_in_milliseconds: u64) {
        self.recent[self.reports_seen % RECENT_REPORTS] = (position, now_in_milliseconds);
        self.reports_seen += 1;
    }

    /// The speed from the earliest report inside the speed window, or the latest before it when none is.
    fn speed_at(&self, position: Point, now_in_milliseconds: u64) -> Point {
        let earlier = self.recent[..self.reports_seen.min(RECENT_REPORTS)]
            .iter()
            .filter(|(_, time)| *time < now_in_milliseconds);
        let inside_window = earlier
            .clone()
            .filter(|(_, time)| now_in_milliseconds - time <= SPEED_WINDOW_MILLISECONDS)
            .min_by_key(|(_, time)| *time);
        let Some(&(from, time)) = inside_window.or_else(|| earlier.max_by_key(|(_, time)| *time)) else {
            return Point::zero();
        };
        let elapsed = (now_in_milliseconds - time).min(i32::MAX as u64 / 1000) as i32;
        (position - from) * 1000 / elapsed
    }
}

/// Whether a speed goes at least the flick speed the way a swipe does.
fn is_fast_along(speed: Point, swipe: Gesture) -> bool {
    let along = match swipe {
        Gesture::SlideUp => -speed.y,
        Gesture::SlideDown => speed.y,
        Gesture::SlideLeft => -speed.x,
        Gesture::SlideRight => speed.x,
        _ => return false,
    };
    along >= FLICK_SPEED_IN_PIXELS_PER_SECOND
}

/// The swipe a movement makes when it goes at least this far, mostly along one direction.
fn swipe_along(moved: Point, distance: i32) -> Gesture {
    let (across, down) = (moved.x.abs(), moved.y.abs());
    let along = across.max(down);
    // The main direction must be half as long again as the other, so a diagonal stroke is no swipe.
    if along < distance || along * 2 < across.min(down) * 3 {
        return Gesture::None;
    }
    match (across > down, moved.x > 0, moved.y > 0) {
        (true, true, _) => Gesture::SlideRight,
        (true, false, _) => Gesture::SlideLeft,
        (false, _, true) => Gesture::SlideDown,
        (false, _, false) => Gesture::SlideUp,
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
            self.wait_for_pulse().await?;
            if let Some(report) = self.read_report().await.map_err(Error::Bus)? {
                return Ok(report);
            }
        }
    }

    /// Waits for the next interrupt pulse, so the caller can note when it came before reading the report.
    pub async fn wait_for_pulse(&mut self) -> Result<(), PanelError<I2cBus, InterruptPin, ResetPin>> {
        self.interrupt_pin
            .wait_for_falling_edge()
            .await
            .map_err(Error::InterruptPin)
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
    ///
    /// The three registers follow each other, so one read takes them all, as a report's does.
    pub async fn read_chip_info(&mut self) -> Result<ChipInfo, I2cBus::Error> {
        let mut registers = [0; 3];
        self.i2c_bus
            .write_read(I2C_ADDRESS, &[CHIP_ID_REGISTER], &mut registers)
            .await?;
        let [chip_id, project_id, firmware_version] = registers;
        Ok(ChipInfo {
            chip_id,
            project_id,
            firmware_version,
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
