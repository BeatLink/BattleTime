extern crate std;

use std::{collections::VecDeque, vec, vec::Vec};

use embassy_futures::block_on;
use embedded_hal::{
    digital::ErrorType,
    i2c::{self, ErrorKind, Operation},
};

use super::*;

// ============================================================================
// Fake hardware
// ============================================================================

#[derive(Debug, PartialEq)]
struct RegisterWrite {
    register: u8,
    value: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FakeBusError;

impl i2c::Error for FakeBusError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

/// A panel that hands out queued reports and register values, and records every register write.
#[derive(Default)]
struct FakeBus {
    queued_reports: VecDeque<[u8; REPORT_LENGTH]>,
    register_values: Vec<RegisterWrite>,
    writes: Vec<RegisterWrite>,
    refuses_writes: bool,
}

impl FakeBus {
    fn value_of(&self, register: u8) -> u8 {
        let stored = self
            .register_values
            .iter()
            .find(|stored| stored.register == register);
        stored.map_or(0, |stored| stored.value)
    }
}

impl I2cErrorType for FakeBus {
    type Error = FakeBusError;
}

impl I2c for FakeBus {
    async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        assert_eq!(address, I2C_ADDRESS);

        match operations {
            [Operation::Write(&[register, value])] => {
                if self.refuses_writes {
                    return Err(FakeBusError);
                }
                self.writes.push(RegisterWrite { register, value });
            }
            [Operation::Write(&[FIRST_REPORT_REGISTER]), Operation::Read(buffer)] => {
                let report = self.queued_reports.pop_front().expect("no report queued");
                buffer.copy_from_slice(&report);
            }
            [Operation::Write(&[register]), Operation::Read(values)] => {
                for (next_register, value) in (register..).zip(values.iter_mut()) {
                    *value = self.value_of(next_register);
                }
            }
            _ => panic!("unexpected I2C transaction"),
        }

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
enum Level {
    High,
    Low,
}

#[derive(Default)]
struct FakeResetPin {
    history: Vec<Level>,
}

impl ErrorType for FakeResetPin {
    type Error = core::convert::Infallible;
}

impl OutputPin for FakeResetPin {
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
struct FakeInterruptPin {
    falling_edges_waited_for: u32,
}

impl ErrorType for FakeInterruptPin {
    type Error = core::convert::Infallible;
}

impl Wait for FakeInterruptPin {
    async fn wait_for_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn wait_for_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn wait_for_rising_edge(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn wait_for_falling_edge(&mut self) -> Result<(), Self::Error> {
        self.falling_edges_waited_for += 1;
        Ok(())
    }

    async fn wait_for_any_edge(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Default)]
struct FakeDelay {
    total_nanoseconds: u64,
}

impl DelayNs for FakeDelay {
    async fn delay_ns(&mut self, nanoseconds: u32) {
        self.total_nanoseconds += nanoseconds as u64;
    }
}

type FakeTouchPanel = TouchPanel<FakeBus, FakeInterruptPin, FakeResetPin>;

fn new_touch_panel(bus: FakeBus) -> FakeTouchPanel {
    TouchPanel::new(bus, FakeInterruptPin::default(), FakeResetPin::default())
}

fn report_registers(gesture_code: u8, finger_count: u8, event_code: u8, column: u16, row: u16) -> [u8; 6] {
    [
        gesture_code,
        finger_count,
        event_code << 6 | (column >> 8) as u8,
        column as u8,
        (row >> 8) as u8,
        row as u8,
    ]
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn new_does_not_talk_to_the_panel() {
    let touch_panel = new_touch_panel(FakeBus::default());

    let parts = touch_panel.release();

    assert!(parts.i2c_bus.writes.is_empty());
    assert!(parts.reset_pin.history.is_empty());
}

#[test]
fn start_resets_the_panel_then_sets_its_interrupts() {
    let mut touch_panel = new_touch_panel(FakeBus::default());
    let mut delay = FakeDelay::default();

    block_on(touch_panel.start(Settings::DEFAULT, &mut delay)).unwrap();

    let parts = touch_panel.release();
    assert_eq!(parts.reset_pin.history, vec![Level::Low, Level::High]);
    assert_eq!(delay.total_nanoseconds, 55_000_000);
    assert_eq!(
        parts.i2c_bus.writes,
        vec![
            RegisterWrite {
                register: MOTION_MASK_REGISTER,
                value: 0b0000_0100
            },
            RegisterWrite {
                register: INTERRUPT_CONTROL_REGISTER,
                value: 0b0111_0000
            },
            RegisterWrite {
                register: AUTO_RESET_REGISTER,
                value: 0
            },
        ]
    );
}

#[test]
fn double_taps_are_reported_only_when_asked_for() {
    let mut touch_panel = new_touch_panel(FakeBus::default());
    let settings = Settings {
        report_double_taps: true,
    };

    block_on(touch_panel.start(settings, &mut FakeDelay::default())).unwrap();

    let writes = touch_panel.release().i2c_bus.writes;
    assert_eq!(
        writes[0],
        RegisterWrite {
            register: MOTION_MASK_REGISTER,
            value: 0b0000_0101
        }
    );
}

#[test]
fn a_report_gives_the_position_gesture_and_finger() {
    let mut bus = FakeBus::default();
    bus.queued_reports
        .push_back(report_registers(0x05, 1, 0, 0x0e7, 0x012));
    let mut touch_panel = new_touch_panel(bus);

    let report = block_on(touch_panel.wait_for_report()).unwrap();

    assert_eq!(
        report,
        TouchReport {
            position: TouchPosition { column: 231, row: 18 },
            gesture: Gesture::Tap,
            finger_event: FingerEvent::PressedDown,
            is_touching: true,
        }
    );
    assert_eq!(Point::from(report.position), Point::new(231, 18));
}

#[test]
fn a_lifted_finger_is_no_longer_touching() {
    let mut bus = FakeBus::default();
    bus.queued_reports
        .push_back(report_registers(0x03, 0, 1, 10, 200));
    let mut touch_panel = new_touch_panel(bus);

    let report = block_on(touch_panel.wait_for_report()).unwrap();

    assert_eq!(report.gesture, Gesture::SlideLeft);
    assert_eq!(report.finger_event, FingerEvent::LiftedUp);
    assert!(!report.is_touching);
}

#[test]
fn garbled_reports_are_skipped() {
    let mut bus = FakeBus::default();
    bus.queued_reports
        .push_back(report_registers(0x05, 1, 2, 240, 10));
    bus.queued_reports
        .push_back(report_registers(0x05, 1, 2, 10, 0xfff));
    bus.queued_reports.push_back(report_registers(0x42, 1, 2, 10, 10));
    bus.queued_reports
        .push_back(report_registers(0x00, 1, 2, 239, 239));
    let mut touch_panel = new_touch_panel(bus);

    let report = block_on(touch_panel.wait_for_report()).unwrap();

    assert_eq!(
        report.position,
        TouchPosition {
            column: 239,
            row: 239
        }
    );
    assert_eq!(report.finger_event, FingerEvent::StillTouching);
    assert_eq!(touch_panel.release().interrupt_pin.falling_edges_waited_for, 4);
}

#[test]
fn chip_info_reads_three_registers() {
    let register_values = vec![
        RegisterWrite {
            register: CHIP_ID_REGISTER,
            value: 0xb4,
        },
        RegisterWrite {
            register: CHIP_ID_REGISTER + 1,
            value: 0x00,
        },
        RegisterWrite {
            register: CHIP_ID_REGISTER + 2,
            value: 0x01,
        },
    ];
    let bus = FakeBus {
        register_values,
        ..FakeBus::default()
    };
    let mut touch_panel = new_touch_panel(bus);

    let chip_info = block_on(touch_panel.read_chip_info()).unwrap();

    assert_eq!(
        chip_info,
        ChipInfo {
            chip_id: 0xb4,
            project_id: 0x00,
            firmware_version: 0x01
        }
    );
}

#[test]
fn sleep_resets_the_panel_then_sends_deep_sleep() {
    let mut touch_panel = new_touch_panel(FakeBus::default());

    block_on(touch_panel.sleep(&mut FakeDelay::default())).unwrap();

    let parts = touch_panel.release();
    assert_eq!(parts.reset_pin.history, vec![Level::Low, Level::High]);
    assert_eq!(
        parts.i2c_bus.writes,
        vec![RegisterWrite {
            register: POWER_MODE_REGISTER,
            value: DEEP_SLEEP
        }]
    );
}

#[test]
fn sleep_holds_the_panel_in_reset_when_it_will_not_listen() {
    let bus = FakeBus {
        refuses_writes: true,
        ..FakeBus::default()
    };
    let mut touch_panel = new_touch_panel(bus);

    block_on(touch_panel.sleep(&mut FakeDelay::default())).unwrap();

    let reset_history = touch_panel.release().reset_pin.history;
    assert_eq!(reset_history, vec![Level::Low, Level::High, Level::Low]);
}

#[test]
fn sleeping_until_a_double_tap_listens_for_nothing_else() {
    let mut touch_panel = new_touch_panel(FakeBus::default());

    block_on(touch_panel.sleep_until_double_tap(&mut FakeDelay::default())).unwrap();

    let writes = touch_panel.release().i2c_bus.writes;
    assert_eq!(
        writes,
        vec![RegisterWrite {
            register: MOTION_MASK_REGISTER,
            value: REPORT_DOUBLE_TAPS
        }]
    );
}

#[test]
fn wake_restores_the_settings_start_was_given() {
    let mut touch_panel = new_touch_panel(FakeBus::default());
    let mut delay = FakeDelay::default();
    let settings = Settings {
        report_double_taps: true,
    };

    block_on(touch_panel.start(settings, &mut delay)).unwrap();
    block_on(touch_panel.sleep(&mut delay)).unwrap();
    block_on(touch_panel.wake(&mut delay)).unwrap();

    let writes = touch_panel.release().i2c_bus.writes;
    let motion_masks: Vec<u8> = writes
        .iter()
        .filter(|write| write.register == MOTION_MASK_REGISTER)
        .map(|write| write.value)
        .collect();
    assert_eq!(motion_masks, vec![0b0000_0101, 0b0000_0101]);
}

#[test]
fn a_bus_error_is_passed_on() {
    let bus = FakeBus {
        refuses_writes: true,
        ..FakeBus::default()
    };
    let mut touch_panel = new_touch_panel(bus);

    let result = block_on(touch_panel.start(Settings::DEFAULT, &mut FakeDelay::default()));

    assert_eq!(result, Err(Error::Bus(FakeBusError)));
}

// ============================================================================
// One finger from landing to lifting
// ============================================================================

/// What the finger makes of a report with this finger event and finger count.
fn finger_report(finger_event: FingerEvent, finger_count: u8) -> TouchReport {
    TouchReport {
        position: TouchPosition {
            column: 120,
            row: 120,
        },
        gesture: Gesture::SlideLeft,
        finger_event,
        is_touching: finger_count > 0,
    }
}

#[test]
fn a_landing_while_the_finger_is_down_is_a_move() {
    let mut finger = Finger::default();

    assert_eq!(
        finger.step(&finger_report(FingerEvent::PressedDown, 1)),
        FingerStep::Lands
    );
    assert_eq!(
        finger.step(&finger_report(FingerEvent::PressedDown, 1)),
        FingerStep::Moves
    );
    assert_eq!(
        finger.step(&finger_report(FingerEvent::StillTouching, 1)),
        FingerStep::Moves
    );
}

#[test]
fn a_finger_lands_again_after_a_lift() {
    let mut finger = Finger::default();
    finger.step(&finger_report(FingerEvent::PressedDown, 1));

    assert_eq!(
        finger.step(&finger_report(FingerEvent::LiftedUp, 0)),
        FingerStep::Lifts
    );
    assert_eq!(
        finger.step(&finger_report(FingerEvent::PressedDown, 1)),
        FingerStep::Lands
    );
}

#[test]
fn a_report_with_no_finger_ends_a_touch_whose_lift_was_lost() {
    let mut finger = Finger::default();
    finger.step(&finger_report(FingerEvent::PressedDown, 1));

    assert_eq!(
        finger.step(&finger_report(FingerEvent::NoEvent, 0)),
        FingerStep::Nothing
    );
    assert_eq!(
        finger.step(&finger_report(FingerEvent::PressedDown, 1)),
        FingerStep::Lands
    );
}

// ============================================================================
// Gestures read from the finger's path
// ============================================================================

/// Lands a finger at a place, then gives every gesture the path's later steps make, one per step.
fn gestures_for(landing: (i32, i32), path: &[(FingerStep, i32, i32, u64)]) -> Vec<Gesture> {
    let mut finder = GestureFinder::default();
    let mut gestures = vec![finder.step(FingerStep::Lands, Point::new(landing.0, landing.1), 0)];
    for &(step, x, y, at) in path {
        gestures.push(finder.step(step, Point::new(x, y), at));
    }
    gestures
}

#[test]
fn a_short_still_touch_is_a_tap() {
    let gestures = gestures_for(
        (120, 120),
        &[
            (FingerStep::Moves, 123, 118, 30),
            (FingerStep::Lifts, 123, 118, 60),
        ],
    );

    assert_eq!(gestures, vec![Gesture::None, Gesture::None, Gesture::Tap]);
}

#[test]
fn a_held_still_finger_is_a_long_press_before_it_lifts() {
    let gestures = gestures_for(
        (120, 120),
        &[
            (FingerStep::Moves, 121, 120, 499),
            (FingerStep::Moves, 121, 120, 500),
            (FingerStep::Moves, 121, 120, 900),
            (FingerStep::Lifts, 121, 120, 1000),
        ],
    );

    assert_eq!(
        gestures,
        vec![
            Gesture::None,
            Gesture::None,
            Gesture::LongPress,
            Gesture::None,
            Gesture::None
        ]
    );
}

#[test]
fn a_long_touch_whose_reports_stopped_is_a_long_press_at_its_lift() {
    let gestures = gestures_for((120, 120), &[(FingerStep::Lifts, 120, 120, 700)]);

    assert_eq!(gestures, vec![Gesture::None, Gesture::LongPress]);
}

#[test]
fn a_swipe_is_found_while_the_finger_is_still_down() {
    let gestures = gestures_for(
        (120, 200),
        &[
            (FingerStep::Moves, 120, 170, 20),
            (FingerStep::Moves, 122, 160, 40),
            (FingerStep::Moves, 124, 100, 60),
            (FingerStep::Lifts, 124, 90, 80),
        ],
    );

    assert_eq!(
        gestures,
        vec![
            Gesture::None,
            Gesture::None,
            Gesture::SlideUp,
            Gesture::None,
            Gesture::None
        ]
    );
}

#[test]
fn each_direction_gives_its_own_swipe() {
    let swipe_to = |x, y| gestures_for((120, 120), &[(FingerStep::Moves, x, y, 50)])[1];

    assert_eq!(swipe_to(120, 60), Gesture::SlideUp);
    assert_eq!(swipe_to(120, 180), Gesture::SlideDown);
    assert_eq!(swipe_to(60, 120), Gesture::SlideLeft);
    assert_eq!(swipe_to(180, 120), Gesture::SlideRight);
}

#[test]
fn a_quick_short_flick_is_a_swipe_when_it_lifts() {
    let gestures = gestures_for(
        (120, 120),
        &[
            (FingerStep::Moves, 100, 121, 60),
            (FingerStep::Lifts, 95, 121, 120),
        ],
    );

    assert_eq!(gestures, vec![Gesture::None, Gesture::None, Gesture::SlideLeft]);
}

#[test]
fn a_slow_short_drag_is_no_gesture() {
    let gestures = gestures_for(
        (120, 120),
        &[
            (FingerStep::Moves, 100, 120, 200),
            (FingerStep::Lifts, 95, 120, 600),
        ],
    );

    assert_eq!(gestures, vec![Gesture::None, Gesture::None, Gesture::None]);
}

#[test]
fn a_diagonal_stroke_is_no_swipe() {
    let gestures = gestures_for(
        (60, 60),
        &[
            (FingerStep::Moves, 120, 110, 50),
            (FingerStep::Lifts, 120, 110, 100),
        ],
    );

    assert_eq!(gestures, vec![Gesture::None, Gesture::None, Gesture::None]);
}

#[test]
fn a_drag_that_stops_still_is_no_long_press() {
    let gestures = gestures_for(
        (120, 120),
        &[
            (FingerStep::Moves, 135, 120, 50),
            (FingerStep::Moves, 135, 120, 800),
        ],
    );

    assert_eq!(gestures, vec![Gesture::None, Gesture::None, Gesture::None]);
}

#[test]
fn a_new_landing_starts_a_new_touch() {
    let mut finder = GestureFinder::default();
    finder.step(FingerStep::Lands, Point::new(120, 120), 0);
    finder.step(FingerStep::Lifts, Point::new(120, 120), 50);

    finder.step(FingerStep::Lands, Point::new(120, 120), 1000);

    assert_eq!(
        finder.step(FingerStep::Lifts, Point::new(120, 120), 1050),
        Gesture::Tap
    );
}

#[test]
fn a_move_whose_landing_was_lost_starts_the_touch_there() {
    let mut finder = GestureFinder::default();

    assert_eq!(
        finder.step(FingerStep::Moves, Point::new(120, 120), 0),
        Gesture::None
    );
    assert_eq!(
        finder.step(FingerStep::Moves, Point::new(120, 60), 40),
        Gesture::SlideUp
    );
}
