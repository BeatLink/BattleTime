use super::*;

const RING: &str = "steps\nstopwatch\nheart\n";

fn everything_installed(_id: &str) -> bool {
    true
}

#[test]
fn next_from_the_face_opens_the_first_app_and_previous_the_last() {
    assert_eq!(
        step(RING, None, Step::Next, everything_installed),
        Destination::App("steps")
    );
    assert_eq!(
        step(RING, None, Step::Previous, everything_installed),
        Destination::App("heart")
    );
}

#[test]
fn steps_move_one_app_along() {
    assert_eq!(
        step(RING, Some("steps"), Step::Next, everything_installed),
        Destination::App("stopwatch")
    );
    assert_eq!(
        step(RING, Some("heart"), Step::Previous, everything_installed),
        Destination::App("stopwatch")
    );
}

#[test]
fn stepping_past_either_end_comes_back_to_the_face() {
    assert_eq!(
        step(RING, Some("heart"), Step::Next, everything_installed),
        Destination::Face
    );
    assert_eq!(
        step(RING, Some("steps"), Step::Previous, everything_installed),
        Destination::Face
    );
}

#[test]
fn apps_that_are_not_installed_are_skipped() {
    let without_stopwatch = |id: &str| id != "stopwatch";

    assert_eq!(
        step(RING, Some("steps"), Step::Next, without_stopwatch),
        Destination::App("heart")
    );
}

#[test]
fn an_app_not_in_the_ring_steps_as_the_face_does() {
    assert_eq!(
        step(RING, Some("calculator"), Step::Next, everything_installed),
        Destination::App("steps")
    );
}

#[test]
fn a_ring_with_nothing_installed_stays_on_the_face() {
    for direction in [Step::Next, Step::Previous] {
        assert_eq!(step(RING, None, direction, |_| false), Destination::Face);
        assert_eq!(step("", None, direction, everything_installed), Destination::Face);
    }
}

#[test]
fn blank_lines_spaces_and_windows_line_ends_are_ignored() {
    let ring = "\r\n  steps \r\n\r\nheart\r\n";

    assert_eq!(
        step(ring, None, Step::Next, everything_installed),
        Destination::App("steps")
    );
    assert_eq!(
        step(ring, Some("steps"), Step::Next, everything_installed),
        Destination::App("heart")
    );
}

#[test]
fn the_default_ring_starts_with_steps() {
    assert_eq!(
        step(DEFAULT_RING, None, Step::Next, everything_installed),
        Destination::App("steps")
    );
}
