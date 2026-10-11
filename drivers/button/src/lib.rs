//! Driver for the PineTime's side button.
//!
//! The button reads on pin P0.13, which is high while the button is held, and only works while
//! pin P0.15 is driven high to power it. Give it a P0.13 input with its pull-down turned on.
//! The driver filters out contact bounce and turns presses into clicks, double clicks and long
//! presses.

#![no_std]

use embassy_futures::select::{select, Either};
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait};

// ============================================================================
// Gestures
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Click,
    DoubleClick,
    /// Reported while the button is still held, as soon as it has been held long enough.
    LongPress,
    /// Reported while the button is still held, after a long press has gone on for longer still.
    LongerPress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub debounce_in_milliseconds: u32,
    pub double_click_window_in_milliseconds: u32,
    pub long_press_in_milliseconds: u32,
    pub longer_press_in_milliseconds: u32,
}

impl Timing {
    pub const DEFAULT: Timing = Timing {
        debounce_in_milliseconds: 10,
        double_click_window_in_milliseconds: 200,
        long_press_in_milliseconds: 400,
        longer_press_in_milliseconds: 2000,
    };
}

impl Default for Timing {
    fn default() -> Timing {
        Timing::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HoldAlreadyReported {
    Nothing,
    LongPress,
    LongerPress,
}

// ============================================================================
// Button
// ============================================================================

pub struct Button<ButtonPin, PowerPin> {
    button_pin: ButtonPin,
    power_pin: PowerPin,
    timing: Timing,
    hold_already_reported: HoldAlreadyReported,
}

impl<ButtonPin, PowerPin> Button<ButtonPin, PowerPin>
where
    ButtonPin: InputPin + Wait,
    PowerPin: OutputPin,
{
    pub fn new(button_pin: ButtonPin, power_pin: PowerPin) -> Result<Self, PowerPin::Error> {
        Button::with_timing(button_pin, power_pin, Timing::DEFAULT)
    }

    pub fn with_timing(
        button_pin: ButtonPin,
        mut power_pin: PowerPin,
        timing: Timing,
    ) -> Result<Self, PowerPin::Error> {
        power_pin.set_high()?;

        Ok(Button {
            button_pin,
            power_pin,
            timing,
            hold_already_reported: HoldAlreadyReported::Nothing,
        })
    }

    /// Reads the pin as it is right now, without waiting out contact bounce.
    pub fn is_pressed(&mut self) -> Result<bool, ButtonPin::Error> {
        self.button_pin.is_high()
    }

    pub async fn wait_for_press(&mut self, delay: &mut impl DelayNs) -> Result<(), ButtonPin::Error> {
        wait_for_steady_level(&mut self.button_pin, Level::Pressed, &self.timing, delay).await
    }

    pub async fn wait_for_release(&mut self, delay: &mut impl DelayNs) -> Result<(), ButtonPin::Error> {
        wait_for_steady_level(&mut self.button_pin, Level::Released, &self.timing, delay).await
    }

    /// Waits for the next gesture; a click is reported only once the double click window has passed.
    pub async fn next_gesture(
        &mut self,
        delay: &mut (impl DelayNs + Clone),
    ) -> Result<Gesture, ButtonPin::Error> {
        let timing = self.timing;

        match self.hold_already_reported {
            HoldAlreadyReported::Nothing => {}
            HoldAlreadyReported::LongPress => {
                let time_left_in_milliseconds = timing
                    .longer_press_in_milliseconds
                    .saturating_sub(timing.long_press_in_milliseconds);
                if !self.released_within(time_left_in_milliseconds, delay).await? {
                    self.hold_already_reported = HoldAlreadyReported::LongerPress;
                    return Ok(Gesture::LongerPress);
                }
                self.hold_already_reported = HoldAlreadyReported::Nothing;
            }
            HoldAlreadyReported::LongerPress => {
                self.wait_for_release(delay).await?;
                self.hold_already_reported = HoldAlreadyReported::Nothing;
            }
        }

        self.wait_for_press(delay).await?;

        if !self
            .released_within(timing.long_press_in_milliseconds, delay)
            .await?
        {
            self.hold_already_reported = HoldAlreadyReported::LongPress;
            return Ok(Gesture::LongPress);
        }

        if self
            .pressed_within(timing.double_click_window_in_milliseconds, delay)
            .await?
        {
            self.wait_for_release(delay).await?;
            return Ok(Gesture::DoubleClick);
        }

        Ok(Gesture::Click)
    }

    pub fn release(self) -> (ButtonPin, PowerPin) {
        (self.button_pin, self.power_pin)
    }

    async fn released_within(
        &mut self,
        time_limit_in_milliseconds: u32,
        delay: &mut (impl DelayNs + Clone),
    ) -> Result<bool, ButtonPin::Error> {
        self.reaches_level_within(Level::Released, time_limit_in_milliseconds, delay)
            .await
    }

    async fn pressed_within(
        &mut self,
        time_limit_in_milliseconds: u32,
        delay: &mut (impl DelayNs + Clone),
    ) -> Result<bool, ButtonPin::Error> {
        self.reaches_level_within(Level::Pressed, time_limit_in_milliseconds, delay)
            .await
    }

    async fn reaches_level_within(
        &mut self,
        level: Level,
        time_limit_in_milliseconds: u32,
        delay: &mut (impl DelayNs + Clone),
    ) -> Result<bool, ButtonPin::Error> {
        let mut timer = delay.clone();
        let reaching_level = wait_for_steady_level(&mut self.button_pin, level, &self.timing, delay);
        let time_running_out = timer.delay_ms(time_limit_in_milliseconds);

        match select(reaching_level, time_running_out).await {
            Either::First(result) => result.map(|()| true),
            Either::Second(()) => Ok(false),
        }
    }
}

// ============================================================================
// Debouncing
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Level {
    Pressed,
    Released,
}

async fn wait_for_steady_level<ButtonPin: InputPin + Wait>(
    button_pin: &mut ButtonPin,
    level: Level,
    timing: &Timing,
    delay: &mut impl DelayNs,
) -> Result<(), ButtonPin::Error> {
    loop {
        match level {
            Level::Pressed => button_pin.wait_for_high().await?,
            Level::Released => button_pin.wait_for_low().await?,
        }

        delay.delay_ms(timing.debounce_in_milliseconds).await;

        let still_at_level = match level {
            Level::Pressed => button_pin.is_high()?,
            Level::Released => button_pin.is_low()?,
        };
        if still_at_level {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests;
