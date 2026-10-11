extern crate std;

use std::{cell::RefCell, pin::pin, rc::Rc, task::Context, vec, vec::Vec};

use core::{
    future::{poll_fn, Future},
    task::{Poll, Waker},
};
use embedded_hal::digital::ErrorType;

use super::*;

// ============================================================================
// Simulated time
// ============================================================================

#[derive(Clone, Copy)]
struct Hold {
    pressed_at_in_microseconds: u64,
    released_at_in_microseconds: u64,
}

#[derive(Default)]
struct Simulation {
    now_in_microseconds: u64,
    holds: Vec<Hold>,
    wake_times_in_microseconds: Vec<u64>,
}

impl Simulation {
    fn is_pressed(&self) -> bool {
        self.holds.iter().any(|hold| {
            hold.pressed_at_in_microseconds <= self.now_in_microseconds
                && self.now_in_microseconds < hold.released_at_in_microseconds
        })
    }

    fn next_change_in_microseconds(&self) -> Option<u64> {
        self.holds
            .iter()
            .flat_map(|hold| [hold.pressed_at_in_microseconds, hold.released_at_in_microseconds])
            .filter(|&time| time > self.now_in_microseconds)
            .min()
    }
}

type SharedSimulation = Rc<RefCell<Simulation>>;

/// Builds a simulation from (pressed at, released at) pairs in milliseconds.
fn simulation_with_holds(holds_in_milliseconds: &[(u64, u64)]) -> SharedSimulation {
    let holds = holds_in_milliseconds
        .iter()
        .map(|&(pressed_at, released_at)| Hold {
            pressed_at_in_microseconds: pressed_at * 1000,
            released_at_in_microseconds: released_at * 1000,
        })
        .collect();

    Rc::new(RefCell::new(Simulation {
        holds,
        ..Simulation::default()
    }))
}

fn now_in_milliseconds(simulation: &SharedSimulation) -> u64 {
    simulation.borrow().now_in_microseconds / 1000
}

/// Polls the future, jumping the clock to the next wake time whenever it has to wait.
fn run<Output>(simulation: &SharedSimulation, future: impl Future<Output = Output>) -> Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());

    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }

        let mut state = simulation.borrow_mut();
        let next_wake_time = state.wake_times_in_microseconds.iter().copied().min();
        state.wake_times_in_microseconds.clear();
        state.now_in_microseconds = next_wake_time.expect("the button would wait forever");
    }
}

// ============================================================================
// Fake hardware
// ============================================================================

struct FakeButtonPin {
    simulation: SharedSimulation,
}

impl FakeButtonPin {
    async fn wait_until_pressed_is(&mut self, pressed: bool) {
        poll_fn(|_| {
            let mut state = self.simulation.borrow_mut();
            if state.is_pressed() == pressed {
                return Poll::Ready(());
            }
            if let Some(change) = state.next_change_in_microseconds() {
                state.wake_times_in_microseconds.push(change);
            }
            Poll::Pending
        })
        .await
    }
}

impl ErrorType for FakeButtonPin {
    type Error = core::convert::Infallible;
}

impl InputPin for FakeButtonPin {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.simulation.borrow().is_pressed())
    }

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.simulation.borrow().is_pressed())
    }
}

impl Wait for FakeButtonPin {
    async fn wait_for_high(&mut self) -> Result<(), Self::Error> {
        self.wait_until_pressed_is(true).await;
        Ok(())
    }

    async fn wait_for_low(&mut self) -> Result<(), Self::Error> {
        self.wait_until_pressed_is(false).await;
        Ok(())
    }

    async fn wait_for_rising_edge(&mut self) -> Result<(), Self::Error> {
        unimplemented!("the driver only waits for levels")
    }

    async fn wait_for_falling_edge(&mut self) -> Result<(), Self::Error> {
        unimplemented!("the driver only waits for levels")
    }

    async fn wait_for_any_edge(&mut self) -> Result<(), Self::Error> {
        unimplemented!("the driver only waits for levels")
    }
}

#[derive(Default)]
struct FakePowerPin {
    history: Vec<bool>,
}

impl ErrorType for FakePowerPin {
    type Error = core::convert::Infallible;
}

impl OutputPin for FakePowerPin {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.history.push(false);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.history.push(true);
        Ok(())
    }
}

#[derive(Clone)]
struct FakeDelay {
    simulation: SharedSimulation,
}

impl DelayNs for FakeDelay {
    async fn delay_ns(&mut self, nanoseconds: u32) {
        let wake_time = self.simulation.borrow().now_in_microseconds + nanoseconds as u64 / 1000;

        poll_fn(|_| {
            let mut state = self.simulation.borrow_mut();
            if state.now_in_microseconds >= wake_time {
                return Poll::Ready(());
            }
            state.wake_times_in_microseconds.push(wake_time);
            Poll::Pending
        })
        .await
    }
}

fn new_button(simulation: &SharedSimulation) -> (Button<FakeButtonPin, FakePowerPin>, FakeDelay) {
    let button_pin = FakeButtonPin {
        simulation: simulation.clone(),
    };
    let button = Button::new(button_pin, FakePowerPin::default()).unwrap();
    let delay = FakeDelay {
        simulation: simulation.clone(),
    };
    (button, delay)
}

fn gestures(holds_in_milliseconds: &[(u64, u64)], count: usize) -> Vec<Gesture> {
    let simulation = simulation_with_holds(holds_in_milliseconds);
    let (mut button, mut delay) = new_button(&simulation);

    (0..count)
        .map(|_| run(&simulation, button.next_gesture(&mut delay)).unwrap())
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn new_powers_the_button() {
    let simulation = simulation_with_holds(&[]);
    let (button, _) = new_button(&simulation);

    let (_, power_pin) = button.release();

    assert_eq!(power_pin.history, vec![true]);
}

#[test]
fn is_pressed_reads_the_pin_right_away() {
    let simulation = simulation_with_holds(&[(0, 50)]);
    let (mut button, _) = new_button(&simulation);

    assert!(button.is_pressed().unwrap());
    simulation.borrow_mut().now_in_microseconds = 50_000;
    assert!(!button.is_pressed().unwrap());
}

#[test]
fn a_short_press_is_a_click_once_the_double_click_window_closes() {
    let simulation = simulation_with_holds(&[(100, 150)]);
    let (mut button, mut delay) = new_button(&simulation);

    let gesture = run(&simulation, button.next_gesture(&mut delay)).unwrap();

    assert_eq!(gesture, Gesture::Click);
    assert_eq!(now_in_milliseconds(&simulation), 150 + 10 + 200);
}

#[test]
fn two_quick_presses_are_a_double_click() {
    assert_eq!(gestures(&[(100, 150), (250, 300)], 1), vec![Gesture::DoubleClick]);
}

#[test]
fn presses_far_apart_are_two_clicks() {
    assert_eq!(
        gestures(&[(100, 150), (600, 650)], 2),
        vec![Gesture::Click, Gesture::Click]
    );
}

#[test]
fn a_long_press_is_reported_while_the_button_is_still_held() {
    let simulation = simulation_with_holds(&[(100, 1000)]);
    let (mut button, mut delay) = new_button(&simulation);

    let gesture = run(&simulation, button.next_gesture(&mut delay)).unwrap();

    assert_eq!(gesture, Gesture::LongPress);
    assert_eq!(now_in_milliseconds(&simulation), 100 + 10 + 400);
}

#[test]
fn letting_go_after_a_long_press_is_not_a_click() {
    assert_eq!(
        gestures(&[(100, 1000), (1500, 1550)], 2),
        vec![Gesture::LongPress, Gesture::Click]
    );
}

#[test]
fn holding_on_reports_a_longer_press_then_waits_for_release() {
    let holds = [(100, 5000), (6000, 6050)];

    let reported = gestures(&holds, 3);

    assert_eq!(
        reported,
        vec![Gesture::LongPress, Gesture::LongerPress, Gesture::Click]
    );
}

#[test]
fn contact_bounce_makes_one_click() {
    let bouncing_press = [(100, 102), (104, 106), (108, 150), (152, 154)];

    assert_eq!(gestures(&bouncing_press, 1), vec![Gesture::Click]);
}

#[test]
fn a_glitch_shorter_than_the_debounce_time_is_ignored() {
    let simulation = simulation_with_holds(&[(100, 103), (500, 550)]);
    let (mut button, mut delay) = new_button(&simulation);

    let gesture = run(&simulation, button.next_gesture(&mut delay)).unwrap();

    assert_eq!(gesture, Gesture::Click);
    assert_eq!(now_in_milliseconds(&simulation), 550 + 10 + 200);
}
