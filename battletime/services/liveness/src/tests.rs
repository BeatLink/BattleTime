extern crate std;

use std::vec::Vec;

use super::*;

fn reading(shell_loops: Option<u32>, radio_events: u32, radio_idle: bool) -> Reading {
    Reading {
        shell_loops,
        radio_events,
        radio_idle,
    }
}

#[test]
fn moving_counters_keep_the_watch_alive_without_pings() {
    let mut liveness = Liveness::new();
    for check in 1..50 {
        let verdict = liveness.check(reading(Some(check), check, false));
        assert_eq!(
            verdict,
            Verdict {
                alive: true,
                ping_shell: false
            }
        );
    }
}

#[test]
fn a_quiet_shell_is_pinged_and_stays_alive_when_it_answers() {
    let mut liveness = Liveness::new();
    let mut loops = 1;
    let mut pings = 0;
    for check in 0..40 {
        let verdict = liveness.check(reading(Some(loops), check, false));
        assert!(verdict.alive);
        if verdict.ping_shell {
            pings += 1;
            loops += 1;
        }
    }
    assert!(pings >= 10, "pinged {pings} times");
}

#[test]
fn a_shell_that_stops_answering_stops_the_feeding() {
    let mut liveness = Liveness::new();
    let verdicts: Vec<Verdict> = (0..SHELL_STOPPED_AFTER + 1)
        .map(|check| liveness.check(reading(Some(7), check, false)))
        .collect();

    assert!(verdicts[..SHELL_STOPPED_AFTER as usize]
        .iter()
        .all(|verdict| verdict.alive));
    assert!(!verdicts[SHELL_STOPPED_AFTER as usize].alive);
    assert!(verdicts[SHELL_PING_AFTER as usize].ping_shell);
}

#[test]
fn no_shell_is_never_waited_for() {
    let mut liveness = Liveness::new();
    for check in 0..100 {
        assert_eq!(
            liveness.check(reading(None, check, false)),
            Verdict {
                alive: true,
                ping_shell: false
            }
        );
    }
}

#[test]
fn a_radio_with_work_that_stops_interrupting_stops_the_feeding() {
    let mut liveness = Liveness::new();
    let verdicts: Vec<bool> = (0..RADIO_STOPPED_AFTER + 1)
        .map(|check| liveness.check(reading(Some(check), 5, false)).alive)
        .collect();

    assert!(verdicts[..RADIO_STOPPED_AFTER as usize]
        .iter()
        .all(|alive| *alive));
    assert!(!verdicts[RADIO_STOPPED_AFTER as usize]);
}

#[test]
fn an_idle_radio_is_never_waited_for_and_starts_afresh() {
    let mut liveness = Liveness::new();
    for check in 0..100 {
        assert!(liveness.check(reading(Some(check), 5, true)).alive);
    }
    // The count it had while idle is where standing still starts from.
    for check in 1..RADIO_STOPPED_AFTER {
        assert!(liveness.check(reading(Some(200 + check), 5, false)).alive);
    }
    assert!(!liveness.check(reading(Some(300), 5, false)).alive);
}
