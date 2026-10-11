extern crate std;

use core::{
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};
use std::{cell::RefCell, rc::Rc, vec, vec::Vec};

use embedded_hal::{digital::ErrorType, pwm};

use super::*;

/// Records every level the driver puts on the motor pin, true for high.
#[derive(Clone, Default)]
struct Pin(Rc<RefCell<Vec<bool>>>);

impl ErrorType for Pin {
    type Error = core::convert::Infallible;
}

impl OutputPin for Pin {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.0.borrow_mut().push(false);
        Ok(())
    }
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.0.borrow_mut().push(true);
        Ok(())
    }
}

/// Records every duty the driver sets, out of 100.
#[derive(Default)]
struct Pwm(Vec<u16>);

impl pwm::ErrorType for Pwm {
    type Error = core::convert::Infallible;
}

impl SetDutyCycle for Pwm {
    fn max_duty_cycle(&self) -> u16 {
        100
    }
    fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
        self.0.push(duty);
        Ok(())
    }
}

/// Adds up the time waited, and can stall forever to stand in for a cancelled buzz.
#[derive(Default)]
struct Delay {
    ns: u64,
    stall: bool,
}

impl DelayNs for Delay {
    async fn delay_ns(&mut self, ns: u32) {
        self.ns += u64::from(ns);
        if self.stall {
            core::future::pending::<()>().await;
        }
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("the test delay never waits"),
    }
}

#[test]
fn new_turns_the_motor_off() {
    let pin = Pin::default();
    Vibrator::new(PinMotor(pin.clone())).unwrap();
    assert_eq!(*pin.0.borrow(), vec![true]);
}

#[test]
fn pulse_runs_the_motor_for_the_time_given() {
    let pin = Pin::default();
    let mut vibrator = Vibrator::new(PinMotor(pin.clone())).unwrap();
    let mut delay = Delay::default();
    block_on(vibrator.pulse(100, 40, &mut delay)).unwrap();
    assert_eq!(*pin.0.borrow(), vec![true, false, true]);
    assert_eq!(delay.ns, 40_000_000);
}

#[test]
fn ring_buzzes_then_rests_for_a_second() {
    let pin = Pin::default();
    let mut vibrator = Vibrator::new(PinMotor(pin.clone())).unwrap();
    let mut delay = Delay::default();
    block_on(vibrator.play(RING, &mut delay)).unwrap();
    assert_eq!(*pin.0.borrow(), vec![true, false, true, true]);
    assert_eq!(delay.ns, 1_000_000_000);
}

#[test]
fn dropping_a_buzz_part_way_turns_the_motor_off() {
    let pin = Pin::default();
    let mut vibrator = Vibrator::new(PinMotor(pin.clone())).unwrap();
    let mut delay = Delay { stall: true, ..Delay::default() };
    {
        let mut buzz = pin!(vibrator.pulse(100, 1000, &mut delay));
        let poll = buzz.as_mut().poll(&mut Context::from_waker(Waker::noop()));
        assert!(poll.is_pending());
        assert_eq!(pin.0.borrow().last(), Some(&false));
    }
    assert_eq!(pin.0.borrow().last(), Some(&true));
}

#[test]
fn pwm_duty_is_the_time_the_motor_rests() {
    let mut vibrator = Vibrator::new(PwmMotor(Pwm::default())).unwrap();
    vibrator.start(30).unwrap();
    vibrator.start(250).unwrap();
    vibrator.stop().unwrap();
    assert_eq!(vibrator.release().0 .0, vec![100, 70, 0, 100]);
}
