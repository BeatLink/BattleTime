//! The vibration motor on P0.16, run by its own task so a pattern never holds up the shell.

use embassy_futures::select::{select, Either};
use embassy_nrf::gpio::Output;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::Delay;
use neotime_app_host::haptics::{Vibration, VibrationPattern};
use neotime_vibrator::{OnOffMotor, Step, Strength, Vibrator, TAP};

/// What the shell asks of the motor; a new vibration cuts the one playing short.
static REQUESTS: Channel<CriticalSectionRawMutex, Vibration, 2> = Channel::new();

/// The alarm pattern plays again and again for up to a minute.
const RING_REPEATS: usize = 60;

/// How long a ring lasts when nothing stops it.
pub const RING_DURATION: embassy_time::Duration = embassy_time::Duration::from_secs(RING_REPEATS as u64);

/// An incoming call rings this pattern again and again for up to two minutes.
const CALL_RING: &[Step] = &[
    Step::buzz(500),
    Step::pause(250),
    Step::buzz(500),
    Step::pause(1250),
];
const CALL_RING_REPEATS: usize = 48;

/// How long a call ring lasts when nothing stops it.
pub const CALL_RING_DURATION: embassy_time::Duration =
    embassy_time::Duration::from_millis(2500 * CALL_RING_REPEATS as u64);

/// One of the phone's `vibrate` buzzes, with the pause after it.
const PHONE_BUZZ: &[Step] = &[Step::buzz(150), Step::pause(150)];

const DOUBLE_TAP: &[Step] = &[Step::buzz(40), Step::pause(80), Step::buzz(40)];
const NOTIFICATION: &[Step] = &[Step::buzz(80), Step::pause(120), Step::buzz(80)];
const ALARM: &[Step] = &[Step::buzz(400), Step::pause(600)];
const SUCCESS: &[Step] = &[Step::buzz(30), Step::pause(60), Step::buzz(80)];
const FAILURE: &[Step] = &[Step::buzz(200), Step::pause(100), Step::buzz(200)];

/// Asks the motor for a vibration; `None`, a vibration do not disturb silenced, does nothing.
pub fn request(vibration: Option<Vibration>) {
    if let Some(vibration) = vibration {
        let _ = REQUESTS.try_send(vibration);
    }
}

fn steps_of(pattern: VibrationPattern) -> &'static [Step] {
    match pattern {
        VibrationPattern::Tap => TAP,
        VibrationPattern::DoubleTap => DOUBLE_TAP,
        VibrationPattern::Notification => NOTIFICATION,
        VibrationPattern::Alarm => ALARM,
        VibrationPattern::Success => SUCCESS,
        VibrationPattern::Failure => FAILURE,
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
                Vibration::Buzz { milliseconds } => {
                    let _ = vibrator.vibrate(Strength::FULL, milliseconds, &mut Delay).await;
                }
                Vibration::Pattern(VibrationPattern::Alarm) => {
                    for _ in 0..RING_REPEATS {
                        let _ = vibrator.play(ALARM, &mut Delay).await;
                    }
                }
                Vibration::Buzzes { count } => {
                    for _ in 0..count {
                        let _ = vibrator.play(PHONE_BUZZ, &mut Delay).await;
                    }
                }
                Vibration::CallRing => {
                    for _ in 0..CALL_RING_REPEATS {
                        let _ = vibrator.play(CALL_RING, &mut Delay).await;
                    }
                }
                Vibration::Pattern(pattern) => {
                    let _ = vibrator.play(steps_of(pattern), &mut Delay).await;
                }
                Vibration::Stop => {}
            }
        };
        next = match select(playing, REQUESTS.receive()).await {
            Either::First(()) => REQUESTS.receive().await,
            Either::Second(vibration) => vibration,
        };
    }
}
