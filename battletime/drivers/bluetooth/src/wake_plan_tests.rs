use super::*;
use crate::packets::Channel;

// ============================================================================
// Helpers
// ============================================================================

const CHANNEL: Channel = Channel {
    index: 37,
    access_address: 0x8E89_BED6,
    crc_initial_value: 0x55_5555,
};

fn transmit_at(start_at: u32) -> RadioOperation {
    RadioOperation::Transmit {
        start_at,
        channel: CHANNEL,
    }
}

fn receive(start_at: u32, give_up_at: u32) -> RadioOperation {
    RadioOperation::Receive {
        start_at,
        give_up_at,
        channel: CHANNEL,
    }
}

fn wake_from(plan: Plan) -> Wake {
    match plan {
        Plan::SleepThenWake(wake) => wake,
        other => panic!("expected a wake, got {other:?}"),
    }
}

// ============================================================================
// Ticks and microseconds
// ============================================================================

#[test]
fn a_second_of_ticks_is_a_million_microseconds() {
    assert_eq!(microseconds_at_tick(0), 0);
    assert_eq!(microseconds_at_tick(1), 30);
    assert_eq!(microseconds_at_tick(32_768), 1_000_000);
    assert_eq!(microseconds_at_tick(3 * 32_768 + 1), 3_000_030);
}

#[test]
fn the_clock_wraps_at_32_bits_without_a_jump() {
    let tick_before_wrap = (1u64 << 32) * 512 / 15_625;
    let before = microseconds_at_tick(tick_before_wrap);
    let after = microseconds_at_tick(tick_before_wrap + 1);
    assert!(before > u32::MAX - 31);
    assert!((30..=31).contains(&after.wrapping_sub(before)));
}

#[test]
fn ticks_within_never_overshoot() {
    for microseconds in [0, 29, 30, 31, 61, 1_000, 1_000_000, 4_000_000] {
        let ticks = ticks_within(microseconds);
        assert!(microseconds_at_tick(ticks) <= microseconds);
        assert!(microseconds_at_tick(ticks + 1) + 1 > microseconds);
    }
}

#[test]
fn the_counter_is_extended_across_its_wrap() {
    assert_eq!(extend_tick(10, 25), 25);
    assert_eq!(extend_tick(0x00FF_FFF0, 0x0000_0010), 0x0100_0010);
    assert_eq!(extend_tick(0x0300_0005, 0x0000_0005), 0x0300_0005);
    assert_eq!(extend_tick(0x02FF_FFFF, 0x0000_0000), 0x0300_0000);
}

// ============================================================================
// Staying awake or sleeping
// ============================================================================

#[test]
fn an_operation_soon_after_another_keeps_the_timer_running() {
    let now = 1_000_000;
    assert_eq!(plan(transmit_at(now + 150), now, 32_768, true), Plan::StayAwake);
    assert_eq!(
        plan(receive(now + SLEEP_THRESHOLD - 1, now + 5_000), now, 32_768, true),
        Plan::StayAwake,
    );
}

#[test]
fn an_operation_already_due_runs_at_once_while_awake() {
    let now = 1_000_000;
    assert_eq!(plan(transmit_at(now - 10), now, 32_768, true), Plan::StayAwake);
}

#[test]
fn turning_the_radio_off_sleeps_with_no_wake() {
    assert_eq!(plan(RadioOperation::Off, 5, 0, true), Plan::Sleep);
    assert_eq!(plan(RadioOperation::Off, 5, 0, false), Plan::Sleep);
}

#[test]
fn the_crystal_gets_about_a_millisecond_to_start() {
    let crystal_start_in_microseconds = microseconds_at_tick(CRYSTAL_START_TICKS);

    assert!((900..1_100).contains(&crystal_start_in_microseconds));
    assert!(microseconds_at_tick(CRYSTAL_START_TICKS + MINIMUM_COUNTER_LEAD_TICKS) < SLEEP_THRESHOLD);
}

#[test]
fn the_timer_wakes_just_before_a_distant_operation() {
    let now_tick = 32_768;
    let now = microseconds_at_tick(now_tick) + 12;
    let start_at = now + 50_000;
    let wake = wake_from(plan(transmit_at(start_at), now, now_tick, true));

    assert_eq!(wake.delay, 0);
    assert_eq!(wake.timer_base, microseconds_at_tick(wake.timer_tick));
    let compare = start_at.wrapping_sub(wake.timer_base);
    assert!(
        (TIMER_LEAD - 1..TIMER_LEAD + 31).contains(&compare),
        "compare {compare}"
    );
    assert_eq!(wake.crystal_tick, wake.timer_tick - CRYSTAL_START_TICKS);
    assert!(wake.crystal_tick >= now_tick + MINIMUM_COUNTER_LEAD_TICKS);
}

#[test]
fn just_past_the_threshold_still_leaves_the_crystal_its_time() {
    let now_tick = 1_000;
    let now = microseconds_at_tick(now_tick + 1) - 1;
    let wake = wake_from(plan(transmit_at(now + SLEEP_THRESHOLD), now, now_tick, true));
    assert_eq!(wake.delay, 0);
    assert!(wake.crystal_tick >= now_tick + MINIMUM_COUNTER_LEAD_TICKS);
}

#[test]
fn a_sleeping_radio_sleeps_on_even_for_a_near_operation() {
    let now_tick = 1_000;
    let now = microseconds_at_tick(now_tick);
    let start_at = now + 500;
    let wake = wake_from(plan(transmit_at(start_at), now, now_tick, false));

    assert_eq!(wake.crystal_tick, now_tick + MINIMUM_COUNTER_LEAD_TICKS);
    assert_eq!(wake.timer_tick, wake.crystal_tick + CRYSTAL_START_TICKS);
    assert_eq!(start_at + wake.delay, wake.timer_base + TIMER_LEAD);
}

#[test]
fn an_overdue_operation_is_moved_to_the_first_moment_after_waking() {
    let now_tick = 70_000;
    let now = microseconds_at_tick(now_tick);
    let start_at = now - 300;
    let wake = wake_from(plan(transmit_at(start_at), now, now_tick, false));
    assert_eq!(start_at.wrapping_add(wake.delay), wake.timer_base + TIMER_LEAD);
}

#[test]
fn planning_works_across_the_clock_wrap() {
    let now_tick = (1u64 << 32) * 512 / 15_625 - 10;
    let now = microseconds_at_tick(now_tick);
    let start_at = now.wrapping_add(100_000);
    let wake = wake_from(plan(transmit_at(start_at), now, now_tick, false));
    assert_eq!(wake.delay, 0);
    let compare = start_at.wrapping_sub(wake.timer_base);
    assert!(
        (TIMER_LEAD - 1..TIMER_LEAD + 31).contains(&compare),
        "compare {compare}"
    );
}

// ============================================================================
// Moving operations
// ============================================================================

#[test]
fn postponing_moves_the_start_and_keeps_the_listening_window() {
    assert_eq!(postpone(receive(100, 400), 25), receive(125, 425));
    assert_eq!(postpone(transmit_at(u32::MAX), 2), transmit_at(1));
    assert_eq!(postpone(RadioOperation::Off, 9), RadioOperation::Off);
}

#[test]
fn only_operations_that_use_the_radio_have_a_start() {
    assert_eq!(start_of(RadioOperation::Off), None);
    assert_eq!(start_of(transmit_at(7)), Some(7));
    assert_eq!(start_of(receive(8, 9)), Some(8));
}
