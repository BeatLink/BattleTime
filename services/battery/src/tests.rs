use super::*;

// ============================================================================
// Helpers
// ============================================================================

fn on_battery(millivolts: i32) -> Measurement {
    Measurement {
        millivolts,
        is_power_present: false,
        is_charge_pin_active: false,
    }
}

fn charging(millivolts: i32) -> Measurement {
    Measurement {
        millivolts,
        is_power_present: true,
        is_charge_pin_active: true,
    }
}

fn charged(millivolts: i32) -> Measurement {
    Measurement {
        millivolts,
        is_power_present: true,
        is_charge_pin_active: false,
    }
}

fn events_of(changes: Changes) -> [bool; 4] {
    BatteryEvent::ALL.map(|event| changes.events.contains(event))
}

/// A battery that has settled at this voltage off the charger.
fn settled_on_battery(millivolts: i32) -> Battery {
    let mut battery = Battery::new();
    for _ in 0..40 {
        battery.measure(on_battery(millivolts));
    }
    battery
}

// ============================================================================
// Readings and the curve
// ============================================================================

#[test]
fn a_reading_becomes_the_battery_voltage() {
    assert_eq!(millivolts_from_reading(0), 0);
    assert_eq!(millivolts_from_reading(-12), 0);
    assert_eq!(millivolts_from_reading(4095), 7200);
    assert_eq!(millivolts_from_reading(2275), 4000);
}

#[test]
fn the_curve_gives_infinitimes_points_and_lines_between_them() {
    assert_eq!(percent_at(3000), 0);
    assert_eq!(percent_at(3500), 0);
    assert_eq!(percent_at(3616), 3);
    assert_eq!(percent_at(3723), 22);
    assert_eq!(percent_at(3776), 48);
    assert_eq!(percent_at(3979), 79);
    assert_eq!(percent_at(4180), 100);
    assert_eq!(percent_at(4300), 100);
    assert_eq!(percent_at(3558), 1);
    assert_eq!(percent_at(4080), 89);
}

// ============================================================================
// Smoothing
// ============================================================================

#[test]
fn the_first_reading_is_taken_as_it_is() {
    let mut battery = Battery::new();
    assert_eq!(battery.millivolts(), 0);

    let changes = battery.measure(on_battery(3979));

    assert_eq!(battery.millivolts(), 3979);
    assert_eq!(battery.percent(), 79);
    assert!(changes.charge_changed);
    assert!(changes.millivolts_changed);
    assert!(changes.events.is_empty());
}

#[test]
fn a_single_dip_moves_the_voltage_only_a_quarter_of_the_way() {
    let mut battery = settled_on_battery(3900);

    battery.measure(on_battery(3700));

    assert_eq!(battery.millivolts(), 3850);
}

#[test]
fn readings_that_stay_lower_bring_the_voltage_down_over_time() {
    let mut battery = settled_on_battery(3900);

    for _ in 0..40 {
        battery.measure(on_battery(3700));
    }

    assert!((3699..=3700).contains(&battery.millivolts()));
}

#[test]
fn a_change_too_small_to_show_is_not_reported() {
    let mut battery = settled_on_battery(3905);

    let changes = battery.measure(on_battery(3906));

    assert!(!changes.millivolts_changed);
    assert!(!changes.charge_changed);
}

// ============================================================================
// A steady charge
// ============================================================================

#[test]
fn off_the_charger_the_charge_never_rises() {
    let mut battery = settled_on_battery(3776);
    assert_eq!(battery.percent(), 48);

    for _ in 0..40 {
        battery.measure(on_battery(3979));
    }

    assert_eq!(battery.percent(), 48);
    assert!(!battery.is_charging());
}

#[test]
fn on_the_charger_the_charge_never_falls() {
    let mut battery = settled_on_battery(3979);
    battery.measure(charging(3979));

    for _ in 0..40 {
        battery.measure(charging(3776));
    }

    assert_eq!(battery.percent(), 79);
    assert!(battery.is_charging());
}

#[test]
fn while_charging_the_charge_stops_at_ninety_nine() {
    let mut battery = settled_on_battery(4100);

    for _ in 0..40 {
        battery.measure(charging(4250));
    }

    assert_eq!(battery.percent(), 99);
    assert!(battery.is_charging());
    assert!(!battery.is_full());
}

#[test]
fn the_charger_staying_on_after_charging_ends_means_full() {
    let mut battery = settled_on_battery(4000);
    battery.measure(charging(4150));

    let first_idle = battery.measure(charged(4150));
    assert!(battery.is_charging());
    assert!(!first_idle.events.contains(BatteryEvent::ChargingCompletes));
    let second_idle = battery.measure(charged(4150));

    assert!(battery.is_full());
    assert!(!battery.is_charging());
    assert_eq!(battery.percent(), 100);
    assert!(second_idle.charge_changed);
    assert_eq!(events_of(second_idle), [false, false, false, true]);
}

#[test]
fn a_top_up_charge_while_full_still_shows_full() {
    let mut battery = settled_on_battery(4150);
    battery.measure(charged(4150));
    battery.measure(charged(4150));

    let changes = battery.measure(charging(4150));

    assert!(battery.is_full());
    assert_eq!(battery.percent(), 100);
    assert!(!changes.charge_changed);
}

#[test]
fn taking_the_charger_off_ends_full_and_the_charge_falls_from_there() {
    let mut battery = settled_on_battery(4150);
    battery.measure(charged(4150));
    battery.measure(charged(4150));

    assert_eq!(battery.percent(), 100);

    battery.measure(on_battery(4150));
    assert!(!battery.is_full());
    assert_eq!(battery.percent(), percent_at(4150));
    for _ in 0..40 {
        battery.measure(on_battery(3979));
    }

    assert_eq!(battery.percent(), 79);
}

#[test]
fn a_watch_started_on_a_full_charger_becomes_full_on_the_second_reading() {
    let mut battery = Battery::new();

    let first = battery.measure(charged(4180));
    assert_eq!(battery.percent(), 99);
    assert!(battery.is_charging());
    assert!(first.events.is_empty());
    battery.measure(charged(4180));

    assert!(battery.is_full());
    assert_eq!(battery.percent(), 100);
}

// ============================================================================
// Events
// ============================================================================

#[test]
fn plugging_in_and_taking_off_the_charger_are_events() {
    let mut battery = settled_on_battery(3900);

    let plugged = battery.measure(charging(3950));
    let unplugged = battery.measure(on_battery(3900));

    assert_eq!(events_of(plugged), [false, true, false, false]);
    assert!(plugged.charge_changed);
    assert_eq!(events_of(unplugged), [false, false, true, false]);
    assert!(unplugged.charge_changed);
}

#[test]
fn starting_on_the_charger_is_no_event() {
    let mut battery = Battery::new();

    let changes = battery.measure(charging(3900));

    assert!(changes.events.is_empty());
}

#[test]
fn going_low_warns_once() {
    let mut battery = settled_on_battery(3723);

    let mut warnings = 0;
    for _ in 0..40 {
        let changes = battery.measure(on_battery(3600));
        warnings += changes.events.contains(BatteryEvent::Low) as u32;
    }

    assert_eq!(battery.percent(), 2);
    assert_eq!(warnings, 1);
}

#[test]
fn a_watch_started_low_warns_at_once() {
    let mut battery = Battery::new();

    let changes = battery.measure(on_battery(3600));

    assert_eq!(events_of(changes), [true, false, false, false]);
}

#[test]
fn the_charger_ends_the_warning_so_the_next_low_warns_again() {
    let mut battery = settled_on_battery(3600);

    battery.measure(charging(3700));
    let unplugged = battery.measure(on_battery(3600));

    assert_eq!(events_of(unplugged), [true, false, true, false]);
    assert!(unplugged
        .events
        .iter()
        .eq([BatteryEvent::Low, BatteryEvent::ChargingStops]));
}

#[test]
fn on_the_charger_a_low_charge_is_no_warning() {
    let mut battery = Battery::new();
    battery.measure(charging(3600));

    let changes = battery.measure(charging(3600));

    assert!(changes.events.is_empty());
}

#[test]
fn the_charger_wakes_the_screen_and_the_warning_keeps_quiet_in_do_not_disturb() {
    for is_do_not_disturb in [false, true] {
        assert!(BatteryEvent::ChargingStarts.wakes_screen(is_do_not_disturb));
        assert!(BatteryEvent::ChargingStops.wakes_screen(is_do_not_disturb));
        assert!(!BatteryEvent::ChargingCompletes.wakes_screen(is_do_not_disturb));
    }
    assert!(BatteryEvent::Low.wakes_screen(false));
    assert!(!BatteryEvent::Low.wakes_screen(true));
}
