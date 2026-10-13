use super::*;

const DRAWING: Foreground = Foreground {
    is_drawing: true,
    is_finger_down: false,
};

const FINGER_DOWN: Foreground = Foreground {
    is_drawing: false,
    is_finger_down: true,
};

#[test]
fn a_quiet_screen_lets_work_start_at_once() {
    assert!(!Foreground::default().is_busy());
    assert!(Foreground::default().may_start(0));
}

#[test]
fn drawing_or_a_finger_down_holds_work_back() {
    assert!(DRAWING.is_busy());
    assert!(FINGER_DOWN.is_busy());
    assert!(!DRAWING.may_start(0));
    assert!(!FINGER_DOWN.may_start(LONGEST_HOLD_IN_MILLISECONDS - 1));
}

#[test]
fn work_held_back_for_the_longest_hold_starts_anyway() {
    let busy = Foreground {
        is_drawing: true,
        is_finger_down: true,
    };

    assert!(busy.may_start(LONGEST_HOLD_IN_MILLISECONDS));
    assert!(busy.may_start(10 * LONGEST_HOLD_IN_MILLISECONDS));
}
