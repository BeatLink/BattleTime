extern crate std;

use std::{vec, vec::Vec};

use embassy_futures::{block_on, select::select};
use embedded_hal::{digital::ErrorType, pwm};

use super::*;

// ============================================================================
// Fake hardware
// ============================================================================

#[derive(Debug, PartialEq)]
enum Level {
    High,
    Low,
}

#[derive(Default)]
struct FakePin {
    history: Vec<Level>,
}

impl ErrorType for FakePin {
    type Error = core::convert::Infallible;
}

impl OutputPin for FakePin {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.history.push(Level::Low);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.history.push(Level::High);
        Ok(())
    }
}

#[derive(Default)]
struct FakePwmChannel {
    duty_history: Vec<u16>,
}

impl pwm::ErrorType for FakePwmChannel {
    type Error = core::convert::Infallible;
}

impl SetDutyCycle for FakePwmChannel {
    fn max_duty_cycle(&self) -> u16 {
        100
    }

    fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
        self.duty_history.push(duty);
        Ok(())
    }
}

#[derive(Default)]
struct FakeDelay {
    total_nanoseconds: u64,
    never_finishes: bool,
}

impl DelayNs for FakeDelay {
    async fn delay_ns(&mut self, nanoseconds: u32) {
        self.total_nanoseconds += nanoseconds as u64;

        if self.never_finishes {
            core::future::pending::<()>().await;
        }
    }
}

fn new_on_off_vibrator() -> Vibrator<OnOffMotor<FakePin>> {
    let motor = OnOffMotor { motor_pin: FakePin::default() };
    Vibrator::new(motor).unwrap()
}

fn pin_history(vibrator: Vibrator<OnOffMotor<FakePin>>) -> Vec<Level> {
    vibrator.release().motor_pin.history
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn new_turns_the_motor_off() {
    let vibrator = new_on_off_vibrator();

    assert_eq!(pin_history(vibrator), vec![Level::High]);
}

#[test]
fn vibrate_runs_the_motor_for_the_time_given() {
    let mut vibrator = new_on_off_vibrator();
    let mut delay = FakeDelay::default();

    block_on(vibrator.vibrate(Strength::FULL, 40, &mut delay)).unwrap();

    assert_eq!(pin_history(vibrator), vec![Level::High, Level::Low, Level::High]);
    assert_eq!(delay.total_nanoseconds, 40_000_000);
}

#[test]
fn ring_buzzes_then_rests_for_the_rest_of_a_second() {
    let mut vibrator = new_on_off_vibrator();
    let mut delay = FakeDelay::default();

    block_on(vibrator.play(RING, &mut delay)).unwrap();

    assert_eq!(pin_history(vibrator), vec![Level::High, Level::Low, Level::High, Level::High]);
    assert_eq!(delay.total_nanoseconds, 1_000_000_000);
}

#[test]
fn cancelling_a_vibration_turns_the_motor_off() {
    let mut vibrator = new_on_off_vibrator();
    let mut delay = FakeDelay { never_finishes: true, ..FakeDelay::default() };

    let long_vibration = vibrator.vibrate(Strength::FULL, 1000, &mut delay);
    let cancel_right_away = core::future::ready(());
    block_on(select(long_vibration, cancel_right_away));

    assert_eq!(pin_history(vibrator), vec![Level::High, Level::Low, Level::High]);
}

#[test]
fn pwm_duty_is_the_share_of_time_the_motor_rests() {
    let motor = PwmMotor { pwm_channel: FakePwmChannel::default() };
    let mut vibrator = Vibrator::new(motor).unwrap();

    vibrator.turn_on(Strength::from_percent(30)).unwrap();
    vibrator.turn_on(Strength::from_percent(250)).unwrap();
    vibrator.turn_off().unwrap();

    let duty_history = vibrator.release().pwm_channel.duty_history;
    assert_eq!(duty_history, vec![100, 70, 0, 100]);
}
