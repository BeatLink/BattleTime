use alloc::boxed::Box;
use core::cell::Cell;

use neotime_runtime::{writer::ImageWriter, AppKind};

use super::*;

// ============================================================================
// A fake system
// ============================================================================

const SYSTEM_NOW: Value = 1_000;
const HOST_NOW: Value = 5_000;
const BATTERY_PERCENT: Value = 73;
const RANDOM_NUMBER: u32 = 42;

/// System values that answer each read with a known value and count the random numbers taken.
#[derive(Default)]
struct FakeValues {
    random_numbers_taken: Cell<u32>,
}

impl SystemValues for FakeValues {
    fn read(&self, value: SystemValue) -> Value {
        match value {
            SystemValue::Now => SYSTEM_NOW,
            SystemValue::BatteryPercent => BATTERY_PERCENT,
            _ => 0,
        }
    }

    fn read_text(&self, _value: SystemValue) -> &str {
        "Song title"
    }

    fn read_element(&self, _value: SystemValue, index: Value) -> Value {
        index * 10
    }

    fn read_element_text(&self, _value: SystemValue, index: Value) -> &str {
        match index {
            0 => "Monday",
            _ => "Tuesday",
        }
    }

    fn random_number(&self) -> u32 {
        self.random_numbers_taken.set(self.random_numbers_taken.get() + 1);
        RANDOM_NUMBER
    }
}

fn clocked(values: &FakeValues) -> Clocked<'_, FakeValues> {
    Clocked {
        values,
        now: HOST_NOW,
    }
}

// ============================================================================
// The clock seen by app code
// ============================================================================

#[test]
fn app_code_sees_the_time_the_host_was_given_rather_than_the_system_clock() {
    let values = FakeValues::default();

    let now = clocked(&values).read(SystemValue::Now);

    assert_eq!(now, HOST_NOW);
}

#[test]
fn other_system_values_come_straight_from_the_system() {
    let values = FakeValues::default();
    let clocked = clocked(&values);

    assert_eq!(clocked.read(SystemValue::BatteryPercent), BATTERY_PERCENT);
    assert_eq!(clocked.read_text(SystemValue::Now), "Song title");
    assert_eq!(clocked.read_element(SystemValue::Now, 3), 30);
    assert_eq!(clocked.read_element_text(SystemValue::Now, 1), "Tuesday");
}

#[test]
fn random_numbers_are_taken_from_the_system() {
    let values = FakeValues::default();
    let clocked = clocked(&values);

    let first = clocked.random_number();
    let second = clocked.random_number();

    assert_eq!((first, second), (RANDOM_NUMBER, RANDOM_NUMBER));
    assert_eq!(values.random_numbers_taken.get(), 2);
}

// ============================================================================
// Input values
// ============================================================================

#[test]
fn swipes_are_numbered_up_down_left_right() {
    let directions = [
        SwipeDirection::Up,
        SwipeDirection::Down,
        SwipeDirection::Left,
        SwipeDirection::Right,
    ];

    let values = directions.map(swipe_value);

    assert_eq!(values, [0, 1, 2, 3]);
}

// ============================================================================
// Effects
// ============================================================================

#[test]
fn an_effect_reads_its_numbers_and_zero_past_its_last_argument() {
    let bytes = ImageWriter::new("toast", "Toast", AppKind::App, 1).build();
    let image = AppImage::parse(&bytes).unwrap();
    let interpreter = Box::new(Interpreter::new());
    let effect = AppEffect {
        kind: EffectKind::Vibrate,
        arguments: [200, 7, 9],
        image: &image,
        interpreter: &interpreter,
    };

    assert_eq!(effect.number(0), 200);
    assert_eq!(effect.number(2), 9);
    assert_eq!(effect.number(MOST_EFFECT_ARGUMENTS), 0);
}

#[test]
fn an_effect_reads_text_from_the_image_and_none_from_a_bad_reference() {
    let mut writer = ImageWriter::new("toast", "Toast", AppKind::App, 1);
    let saved = writer.string("Saved!");
    let bytes = writer.build();
    let image = AppImage::parse(&bytes).unwrap();
    let interpreter = Box::new(Interpreter::new());
    let effect = AppEffect {
        kind: EffectKind::Go,
        arguments: [saved as Value, -1, 0],
        image: &image,
        interpreter: &interpreter,
    };

    assert_eq!(effect.text(0), Some("Saved!"));
    assert_eq!(effect.text(1), None);
}
