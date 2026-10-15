use crate::link_pace::*;

// ============================================================================
// Settling after a connection
// ============================================================================

#[test]
fn a_new_connection_asks_for_nothing_until_it_settles() {
    let mut pace = LinkPace::new(1);

    assert_eq!(pace.request(1), None);
    assert_eq!(pace.request(5), None);
    assert_eq!(pace.next_change_seconds(), Some(6));
}

#[test]
fn a_settled_connection_asks_once_for_the_idle_parameters() {
    let mut pace = LinkPace::new(1);

    assert_eq!(pace.request(6), Some(IDLE_PARAMETERS));
    assert_eq!(pace.request(7), None);
    assert_eq!(pace.next_change_seconds(), None);
}

// ============================================================================
// Transfers
// ============================================================================

#[test]
fn a_transfer_asks_for_the_busy_parameters_at_once() {
    let mut pace = LinkPace::new(0);
    pace.note_transfer(1);

    assert_eq!(pace.request(1), Some(BUSY_PARAMETERS));
    assert_eq!(pace.next_change_seconds(), Some(6));
}

#[test]
fn more_of_the_same_transfer_asks_nothing_new() {
    let mut pace = LinkPace::new(0);
    pace.note_transfer(10);
    assert_eq!(pace.request(10), Some(BUSY_PARAMETERS));

    pace.note_transfer(12);

    assert_eq!(pace.request(12), None);
    assert_eq!(pace.next_change_seconds(), Some(17));
}

#[test]
fn the_link_slows_again_after_the_last_transfer_settles() {
    let mut pace = LinkPace::new(0);
    assert_eq!(pace.request(5), Some(IDLE_PARAMETERS));
    pace.note_transfer(20);
    assert_eq!(pace.request(20), Some(BUSY_PARAMETERS));

    assert_eq!(pace.request(24), None);
    assert_eq!(pace.request(25), Some(IDLE_PARAMETERS));
    assert_eq!(pace.request(26), None);
}

#[test]
fn a_transfer_that_settles_before_any_request_asks_only_for_idle() {
    let mut pace = LinkPace::new(0);
    pace.note_transfer(1);

    assert_eq!(pace.request(9), Some(IDLE_PARAMETERS));
}

// ============================================================================
// Requests that never went out
// ============================================================================

#[test]
fn an_idle_request_that_never_went_out_is_asked_again_a_second_later() {
    let mut pace = LinkPace::new(0);
    assert_eq!(pace.request(5), Some(IDLE_PARAMETERS));

    pace.retry_later(5);

    assert_eq!(pace.next_change_seconds(), Some(6));
    assert_eq!(pace.request(5), None);
    assert_eq!(pace.request(6), Some(IDLE_PARAMETERS));
}

#[test]
fn a_busy_request_that_never_went_out_is_asked_again_at_once() {
    let mut pace = LinkPace::new(0);
    pace.note_transfer(10);
    assert_eq!(pace.request(10), Some(BUSY_PARAMETERS));

    pace.retry_later(10);

    assert_eq!(pace.request(10), Some(BUSY_PARAMETERS));
    assert_eq!(pace.next_change_seconds(), Some(15));
}
