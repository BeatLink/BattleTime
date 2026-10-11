//! The backlight: three pins, each turned on by pulling it low, set through `neotime-display`'s
//! `Backlight`, plus a dim level for an always-on screen that pulses the low pin in hardware.
//!
//! The dim level runs without the CPU: RTC2 counts at 32768 Hz, one PPI channel turns the low pin
//! off at the end of its short on time, and another turns it back on and restarts the count, as
//! InfiniTime's always-on brightness does.

use core::ptr::NonNull;

use embassy_nrf::{
    gpio::Output,
    gpiote::OutputChannel,
    peripherals::{PPI_CH1, PPI_CH2, RTC2},
    ppi::{Event, Ppi, Task},
    rtc::{CompareChannel, Interrupt, Rtc},
    Peri,
};
use embedded_hal::digital::{ErrorType, OutputPin};
use neotime_display::BacklightLevel;
use nrf_pac as pac;

// ============================================================================
// Levels
// ============================================================================

pub const DEFAULT_LEVEL: u8 = 2;

/// Whether the screen dims to the panel's low-power mode instead of turning off.
pub const ALWAYS_ON_SCREEN: bool = cfg!(feature = "always-on-screen");

/// RTC2 ticks in one pulse of the dim level, about 1 kHz, so it does not flicker.
const PULSE_PERIOD_TICKS: u32 = 32;

/// RTC2 ticks the low pin stays lit in each pulse, a tenth of the low level.
const PULSE_LIT_TICKS: u32 = 3;

fn level_for(app_level: u8) -> BacklightLevel {
    match app_level {
        1 => BacklightLevel::Low,
        2 => BacklightLevel::Medium,
        _ => BacklightLevel::High,
    }
}

// ============================================================================
// Pins
// ============================================================================

/// The low pin is driven by a GPIOTE channel, so the pulse can reach it; the others are plain pins.
pub enum BacklightPin {
    Plain(Output<'static>),
    Pulsed(OutputChannel<'static>),
}

impl ErrorType for BacklightPin {
    type Error = core::convert::Infallible;
}

impl OutputPin for BacklightPin {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        match self {
            BacklightPin::Plain(pin) => pin.set_low(),
            BacklightPin::Pulsed(channel) => channel.clear(),
        }
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        match self {
            BacklightPin::Plain(pin) => pin.set_high(),
            BacklightPin::Pulsed(channel) => channel.set(),
        }
        Ok(())
    }
}

// ============================================================================
// Backlight
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    On,
    Off,
    Dim,
}

pub struct Backlight {
    pins: neotime_display::Backlight<BacklightPin>,
    pulse_counter: Rtc<'static>,
    turn_off_channel: Ppi<'static, PPI_CH1, 1, 1>,
    turn_on_channel: Ppi<'static, PPI_CH2, 1, 2>,
    level: u8,
    state: State,
}

impl Backlight {
    /// Takes the pins and the pulse hardware and turns the light on at the default level.
    pub fn new(
        low: OutputChannel<'static>,
        middle: Output<'static>,
        high: Output<'static>,
        rtc: Peri<'static, RTC2>,
        turn_off_ppi: Peri<'static, PPI_CH1>,
        turn_on_ppi: Peri<'static, PPI_CH2>,
    ) -> Backlight {
        let mut pulse_counter = Rtc::new(rtc, 0).unwrap();
        let _ = pulse_counter.set_compare(CompareChannel::_0, PULSE_LIT_TICKS);
        let _ = pulse_counter.set_compare(CompareChannel::_1, PULSE_PERIOD_TICKS);
        pulse_counter.enable_event(Interrupt::Compare0);
        pulse_counter.enable_event(Interrupt::Compare1);

        let turn_off_channel = Ppi::new_one_to_one(turn_off_ppi, compare_event(0), low.task_set());
        let turn_on_channel =
            Ppi::new_one_to_two(turn_on_ppi, compare_event(1), low.task_clr(), restart_task());

        let Ok(pins) = neotime_display::Backlight::new(
            BacklightPin::Pulsed(low),
            BacklightPin::Plain(middle),
            BacklightPin::Plain(high),
            level_for(DEFAULT_LEVEL),
        );

        Backlight {
            pins,
            pulse_counter,
            turn_off_channel,
            turn_on_channel,
            level: DEFAULT_LEVEL,
            state: State::On,
        }
    }

    /// The level, from 1 to 3.
    pub fn level(&self) -> u8 {
        self.level
    }

    pub fn set_level(&mut self, level: u8) {
        self.level = level.clamp(1, 3);
        if self.state == State::On {
            let _ = self.pins.set_level(level_for(self.level));
        }
    }

    pub fn turn_on(&mut self) {
        self.stop_pulse();
        let _ = self.pins.set_level(level_for(self.level));
        self.state = State::On;
    }

    pub fn turn_off(&mut self) {
        self.stop_pulse();
        let _ = self.pins.set_level(BacklightLevel::Off);
        self.state = State::Off;
    }

    /// Lights the screen faintly, for an always-on screen.
    pub fn dim(&mut self) {
        if self.state == State::Dim {
            return;
        }
        let _ = self.pins.set_level(BacklightLevel::Low);
        self.pulse_counter.clear();
        self.turn_off_channel.enable();
        self.turn_on_channel.enable();
        self.pulse_counter.enable();
        self.state = State::Dim;
    }

    /// Stops the pulse before anything else drives the low pin, so a late pulse cannot undo it.
    fn stop_pulse(&mut self) {
        if self.state != State::Dim {
            return;
        }
        self.turn_off_channel.disable();
        self.turn_on_channel.disable();
        self.pulse_counter.disable();
    }
}

// ============================================================================
// RTC2's registers for the PPI channels
// ============================================================================

fn compare_event(compare: usize) -> Event<'static> {
    let register = pac::RTC2.events_compare(compare).as_ptr();
    // SAFETY: the pointer is RTC2's own EVENTS_COMPARE register.
    unsafe { Event::new_unchecked(NonNull::new_unchecked(register)) }
}

fn restart_task() -> Task<'static> {
    let register = pac::RTC2.tasks_clear().as_ptr();
    // SAFETY: the pointer is RTC2's own TASKS_CLEAR register.
    unsafe { Task::new_unchecked(NonNull::new_unchecked(register)) }
}
