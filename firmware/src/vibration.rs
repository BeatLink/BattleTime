//! The vibration motor on P0.16, run by its own task so a pattern never holds up the shell.

use embassy_futures::select::{select, Either};
use embassy_nrf::gpio::Output;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::Delay;
use neotime_vibrator::{OnOffMotor, Step, Strength, Vibrator, TAP};

/// What the shell asks of the motor; a new request cuts the one playing short.
#[derive(Clone, Copy)]
pub enum Request {
    Buzz {
        milliseconds: u16,
    },
    Pattern(&'static [Step]),
    /// A ringing pattern, played again and again for up to a minute.
    Ring(&'static [Step]),
    Stop,
}

pub static REQUESTS: Channel<CriticalSectionRawMutex, Request, 2> = Channel::new();

const LONGEST_BUZZ_IN_MILLISECONDS: u16 = 2000;
const RING_REPEATS: usize = 60;

const DOUBLE_TAP: &[Step] = &[Step::buzz(40), Step::pause(80), Step::buzz(40)];
const NOTIFICATION: &[Step] = &[Step::buzz(80), Step::pause(120), Step::buzz(80)];
const ALARM: &[Step] = &[Step::buzz(400), Step::pause(600)];
const SUCCESS: &[Step] = &[Step::buzz(30), Step::pause(60), Step::buzz(80)];
const FAILURE: &[Step] = &[Step::buzz(200), Step::pause(100), Step::buzz(200)];

/// The request for `vibrate_pattern`'s numbered names: tap, double tap, notification, alarm, success, failure.
pub fn named_pattern(number: i64) -> Option<Request> {
    Some(match number {
        0 => Request::Pattern(TAP),
        1 => Request::Pattern(DOUBLE_TAP),
        2 => Request::Pattern(NOTIFICATION),
        3 => Request::Ring(ALARM),
        4 => Request::Pattern(SUCCESS),
        5 => Request::Pattern(FAILURE),
        _ => return None,
    })
}

pub fn buzz(milliseconds: i64) -> Request {
    Request::Buzz {
        milliseconds: milliseconds.clamp(0, LONGEST_BUZZ_IN_MILLISECONDS as i64) as u16,
    }
}

#[embassy_executor::task]
pub async fn run_vibrator(motor_pin: Output<'static>) {
    // A plain pin never fails to set.
    let Ok(mut vibrator) = Vibrator::new(OnOffMotor { motor_pin });
    let mut next = REQUESTS.receive().await;
    loop {
        let playing = async {
            match next {
                Request::Buzz { milliseconds } => {
                    let _ = vibrator.vibrate(Strength::FULL, milliseconds, &mut Delay).await;
                }
                Request::Pattern(pattern) => {
                    let _ = vibrator.play(pattern, &mut Delay).await;
                }
                Request::Ring(pattern) => {
                    for _ in 0..RING_REPEATS {
                        let _ = vibrator.play(pattern, &mut Delay).await;
                    }
                }
                Request::Stop => {}
            }
        };
        next = match select(playing, REQUESTS.receive()).await {
            Either::First(()) => REQUESTS.receive().await,
            Either::Second(request) => request,
        };
    }
}
