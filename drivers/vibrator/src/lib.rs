//! Driver for the PineTime's vibration motor.
//!
//! The motor hangs off P0.16 through a transistor and runs while the pin is low. A plain pin gives
//! it full power or none; a PWM channel on the same pin sets how strongly it buzzes.
//!
//! A buzz in progress always ends with the motor off, even when its future is dropped part way.

#![no_std]

use embedded_hal::{digital::OutputPin, pwm::SetDutyCycle};
use embedded_hal_async::delay::DelayNs;

/// Something that can spin the motor at a strength from 0 (off) to 100 percent.
pub trait Motor {
    /// What goes wrong when the hardware refuses a change.
    type Error;

    /// Sets the strength in percent; anything above 100 counts as 100.
    fn set(&mut self, strength: u8) -> Result<(), Self::Error>;
}

/// The motor on a plain output pin, where any strength above zero means full power.
pub struct PinMotor<P>(pub P);

impl<P: OutputPin> Motor for PinMotor<P> {
    type Error = P::Error;

    fn set(&mut self, strength: u8) -> Result<(), Self::Error> {
        if strength == 0 {
            self.0.set_high()
        } else {
            self.0.set_low()
        }
    }
}

/// The motor on a PWM channel whose output is high for the duty fraction, as is the default.
pub struct PwmMotor<P>(pub P);

impl<P: SetDutyCycle> Motor for PwmMotor<P> {
    type Error = P::Error;

    fn set(&mut self, strength: u8) -> Result<(), Self::Error> {
        // The motor runs while the pin is low, so the duty is the time it rests.
        self.0.set_duty_cycle_percent(100 - strength.min(100))
    }
}

/// One part of a vibration pattern: a strength held for a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// Strength in percent, where 0 is a pause.
    pub strength: u8,
    /// How long the step lasts, in milliseconds.
    pub ms: u16,
}

impl Step {
    /// A full-strength buzz.
    pub const fn buzz(ms: u16) -> Self {
        Self { strength: 100, ms }
    }

    /// A pause with the motor off.
    pub const fn pause(ms: u16) -> Self {
        Self { strength: 0, ms }
    }
}

/// A short tap, as wasp-os gives for a button or a notification.
pub const TAP: &[Step] = &[Step::buzz(40)];

/// One second of an incoming call or alarm, as InfiniTime rings it; play it in a loop.
pub const RING: &[Step] = &[Step::buzz(50), Step::pause(950)];

/// The vibration motor.
pub struct Vibrator<M: Motor> {
    motor: M,
}

impl<M: Motor> Vibrator<M> {
    /// Takes the motor and makes sure it is off.
    pub fn new(mut motor: M) -> Result<Self, M::Error> {
        motor.set(0)?;
        Ok(Self { motor })
    }

    /// Starts the motor at `strength` percent until [`stop`](Self::stop) is called.
    pub fn start(&mut self, strength: u8) -> Result<(), M::Error> {
        self.motor.set(strength)
    }

    /// Stops the motor.
    pub fn stop(&mut self) -> Result<(), M::Error> {
        self.motor.set(0)
    }

    /// Buzzes at `strength` percent for `ms` milliseconds.
    pub async fn pulse(&mut self, strength: u8, ms: u32, delay: &mut impl DelayNs) -> Result<(), M::Error> {
        let mut running = Running(Some(&mut self.motor));
        running.set(strength)?;
        delay.delay_ms(ms).await;
        running.finish()
    }

    /// Plays a pattern once, then leaves the motor off.
    pub async fn play(&mut self, pattern: &[Step], delay: &mut impl DelayNs) -> Result<(), M::Error> {
        let mut running = Running(Some(&mut self.motor));
        for step in pattern {
            running.set(step.strength)?;
            delay.delay_ms(step.ms.into()).await;
        }
        running.finish()
    }

    /// Gives back the motor, leaving it as it is.
    pub fn release(self) -> M {
        self.motor
    }
}

/// Holds the motor during a buzz and turns it off if the buzz is dropped before it ends.
struct Running<'a, M: Motor>(Option<&'a mut M>);

impl<M: Motor> Running<'_, M> {
    fn set(&mut self, strength: u8) -> Result<(), M::Error> {
        match self.0.as_mut() {
            Some(motor) => motor.set(strength),
            None => Ok(()),
        }
    }

    fn finish(mut self) -> Result<(), M::Error> {
        match self.0.take() {
            Some(motor) => motor.set(0),
            None => Ok(()),
        }
    }
}

impl<M: Motor> Drop for Running<'_, M> {
    fn drop(&mut self) {
        if let Some(motor) = self.0.take() {
            let _ = motor.set(0);
        }
    }
}

#[cfg(test)]
mod tests;
