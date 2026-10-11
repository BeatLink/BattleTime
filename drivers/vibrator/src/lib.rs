//! Driver for the PineTime's vibration motor, which runs while pin P0.16 is held low.

#![no_std]

use embedded_hal::{digital::OutputPin, pwm::SetDutyCycle};
use embedded_hal_async::delay::DelayNs;

// ============================================================================
// Strength
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Strength {
    percent: u8,
}

impl Strength {
    pub const OFF: Strength = Strength { percent: 0 };
    pub const FULL: Strength = Strength { percent: 100 };

    pub const fn from_percent(percent: u8) -> Strength {
        if percent > 100 {
            Strength::FULL
        } else {
            Strength { percent }
        }
    }

    pub const fn percent(self) -> u8 {
        self.percent
    }

    pub const fn is_off(self) -> bool {
        self.percent == 0
    }
}

// ============================================================================
// Motor wiring
// ============================================================================

pub trait Motor {
    type Error;

    fn set_strength(&mut self, strength: Strength) -> Result<(), Self::Error>;
}

/// A motor on a plain pin, which is either fully on or off.
pub struct OnOffMotor<MotorPin> {
    pub motor_pin: MotorPin,
}

impl<MotorPin: OutputPin> Motor for OnOffMotor<MotorPin> {
    type Error = MotorPin::Error;

    fn set_strength(&mut self, strength: Strength) -> Result<(), Self::Error> {
        if strength.is_off() {
            self.motor_pin.set_high()
        } else {
            self.motor_pin.set_low()
        }
    }
}

/// A motor on a PWM channel, which can run at any strength.
pub struct PwmMotor<PwmChannel> {
    pub pwm_channel: PwmChannel,
}

impl<PwmChannel: SetDutyCycle> Motor for PwmMotor<PwmChannel> {
    type Error = PwmChannel::Error;

    fn set_strength(&mut self, strength: Strength) -> Result<(), Self::Error> {
        let percent_of_time_pin_is_high = 100 - strength.percent();
        self.pwm_channel.set_duty_cycle_percent(percent_of_time_pin_is_high)
    }
}

// ============================================================================
// Patterns
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub strength: Strength,
    pub duration_in_milliseconds: u16,
}

impl Step {
    pub const fn buzz(duration_in_milliseconds: u16) -> Step {
        Step { strength: Strength::FULL, duration_in_milliseconds }
    }

    pub const fn pause(duration_in_milliseconds: u16) -> Step {
        Step { strength: Strength::OFF, duration_in_milliseconds }
    }
}

pub const TAP: &[Step] = &[Step::buzz(40)];

/// One second of ringing for a call or alarm; play it again to keep ringing.
pub const RING: &[Step] = &[Step::buzz(50), Step::pause(950)];

// ============================================================================
// Vibrator
// ============================================================================

pub struct Vibrator<MotorDriver: Motor> {
    motor: MotorDriver,
}

impl<MotorDriver: Motor> Vibrator<MotorDriver> {
    pub fn new(mut motor: MotorDriver) -> Result<Self, MotorDriver::Error> {
        motor.set_strength(Strength::OFF)?;
        Ok(Vibrator { motor })
    }

    pub fn turn_on(&mut self, strength: Strength) -> Result<(), MotorDriver::Error> {
        self.motor.set_strength(strength)
    }

    pub fn turn_off(&mut self) -> Result<(), MotorDriver::Error> {
        self.motor.set_strength(Strength::OFF)
    }

    pub async fn vibrate(
        &mut self,
        strength: Strength,
        duration_in_milliseconds: u16,
        delay: &mut impl DelayNs,
    ) -> Result<(), MotorDriver::Error> {
        let single_step = [Step { strength, duration_in_milliseconds }];
        self.play(&single_step, delay).await
    }

    pub async fn play(&mut self, pattern: &[Step], delay: &mut impl DelayNs) -> Result<(), MotorDriver::Error> {
        let guard = TurnsMotorOffWhenDropped { motor: &mut self.motor };

        for step in pattern {
            guard.motor.set_strength(step.strength)?;
            delay.delay_ms(step.duration_in_milliseconds as u32).await;
        }

        Ok(())
    }

    pub fn release(self) -> MotorDriver {
        self.motor
    }
}

// ============================================================================
// Turning the motor off when a pattern is cancelled
// ============================================================================

struct TurnsMotorOffWhenDropped<'a, MotorDriver: Motor> {
    motor: &'a mut MotorDriver,
}

impl<MotorDriver: Motor> Drop for TurnsMotorOffWhenDropped<'_, MotorDriver> {
    fn drop(&mut self) {
        let _ = self.motor.set_strength(Strength::OFF);
    }
}

#[cfg(test)]
mod tests;
