//! The touch panel, turned into the touches the app host takes.

use embassy_futures::select::{select, Either};
use embassy_nrf::gpio::{Input, Output};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};
use embassy_time::{Delay, Duration, Timer};
use neotime_app_host::Touch;
use neotime_runtime::events::SwipeDirection;
use neotime_settings::wake_on_touch;
use neotime_touch::{FingerEvent, Gesture, Settings, TouchPanel, TouchReport};

use crate::{
    diagnostics::{self, Counter},
    foreground, I2cDeviceOnBus,
};

/// Touches waiting for the UI, which merges queued drags.
pub static TOUCHES: Channel<CriticalSectionRawMutex, Touch, 4> = Channel::new();

/// Places in the queue drags never take, so a lift and its gesture always fit.
const PLACES_KEPT_FOR_LIFTS: usize = 2;

/// What the panel listens for, set by the shell as the screen turns on and off.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Listening {
    Everything,
    /// The screen is dark, and the `wake_on_touch` setting says which touch should still reach the shell.
    WhileDark {
        wake_on_touch: i32,
    },
}

static LISTENING: Signal<CriticalSectionRawMutex, Listening> = Signal::new();

/// Tells the touch task what to listen for; only the newest request counts.
pub fn listen_for(listening: Listening) {
    LISTENING.signal(listening);
}

/// Signalled once the panel has started, or given up starting.
pub static STARTED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

const START_ATTEMPTS: usize = 5;
const RETRY_DELAY: Duration = Duration::from_millis(100);

/// How far a finger moves before a touch counts as a drag.
const DRAG_THRESHOLD_IN_PIXELS: i32 = 10;

/// Follows one finger from press to lift, so each touch gives at most one gesture.
#[derive(Default)]
struct Finger {
    start: Option<(i32, i32)>,
    dragging: bool,
    gesture_sent: bool,
}

impl Finger {
    fn touches_for(&mut self, report: &TouchReport, mut send: impl FnMut(Touch)) {
        let x = report.position.column as i32;
        let y = report.position.row as i32;
        match report.finger_event {
            FingerEvent::PressedDown => {
                *self = Finger {
                    start: Some((x, y)),
                    ..Finger::default()
                };
                foreground::set_finger_down(true);
                send(Touch::Press { x, y });
            }
            FingerEvent::StillTouching => {
                let (start_x, start_y) = *self.start.get_or_insert((x, y));
                let moved = (x - start_x).abs().max((y - start_y).abs());
                if self.dragging || moved > DRAG_THRESHOLD_IN_PIXELS {
                    self.dragging = true;
                    send(Touch::Drag {
                        x,
                        y,
                        start_x,
                        start_y,
                    });
                }
            }
            FingerEvent::LiftedUp => {
                foreground::set_finger_down(false);
                send(Touch::Release { x, y })
            }
            FingerEvent::NoEvent => {}
        }

        if self.gesture_sent {
            return;
        }
        let gesture = match report.gesture {
            Gesture::Tap if !self.dragging => Touch::Click { x, y },
            Gesture::DoubleTap => Touch::DoubleClick { x, y },
            Gesture::LongPress if !self.dragging => Touch::LongPress { x, y },
            Gesture::SlideUp => Touch::Swipe(SwipeDirection::Up),
            Gesture::SlideDown => Touch::Swipe(SwipeDirection::Down),
            Gesture::SlideLeft => Touch::Swipe(SwipeDirection::Left),
            Gesture::SlideRight => Touch::Swipe(SwipeDirection::Right),
            _ => return,
        };
        self.gesture_sent = true;
        send(gesture);
    }
}

/// Queues touches for the UI, holding back the newest drag while the queue is nearly full.
#[derive(Default)]
struct TouchSender {
    held_drag: Option<Touch>,
}

impl TouchSender {
    fn send(&mut self, touch: Touch) {
        if let Touch::Drag { .. } = touch {
            self.held_drag = Some(touch);
            self.send_held_drag();
            return;
        }
        // A drag still held when the finger lifts is out of date, so it goes now or not at all.
        self.send_held_drag();
        self.held_drag = None;
        let _ = TOUCHES.try_send(touch);
    }

    fn send_held_drag(&mut self) {
        if TOUCHES.free_capacity() > PLACES_KEPT_FOR_LIFTS {
            if let Some(drag) = self.held_drag.take() {
                let _ = TOUCHES.try_send(drag);
            }
        }
    }
}

#[embassy_executor::task]
pub async fn read_touches(
    i2c_bus: I2cDeviceOnBus,
    interrupt_pin: Input<'static>,
    reset_pin: Output<'static>,
) {
    let mut panel = TouchPanel::new(i2c_bus, interrupt_pin, reset_pin);
    // Double taps are left off, because the panel would hold back every single tap waiting for one.
    let mut started = false;
    for _ in 0..START_ATTEMPTS {
        if panel.start(Settings::DEFAULT, &mut Delay).await.is_ok() {
            started = true;
            break;
        }
        Timer::after(RETRY_DELAY).await;
    }
    // The accelerometer shares the bus and its reset upsets the panel's start, so it waits for this.
    STARTED.signal(());
    if !started {
        diagnostics::set(Counter::TouchState, 2);
        return;
    }
    diagnostics::set(Counter::TouchState, 1);
    let mut finger = Finger::default();
    let mut sender = TouchSender::default();
    let mut listening = Listening::Everything;
    loop {
        let report = match select(panel.wait_for_report(), LISTENING.wait()).await {
            Either::First(Ok(report)) => report,
            Either::First(Err(_)) => continue,
            Either::Second(next) => {
                if next != listening {
                    let _ = switch_panel(&mut panel, listening, next).await;
                    listening = next;
                    finger = Finger::default();
                }
                continue;
            }
        };
        diagnostics::add_one(Counter::TouchReports);
        // Asleep for double taps, the panel interrupts only for one, so any report means a double tap.
        if listening
            == (Listening::WhileDark {
                wake_on_touch: wake_on_touch::DOUBLE_TAP,
            })
        {
            let x = report.position.column as i32;
            let y = report.position.row as i32;
            sender.send(Touch::DoubleClick { x, y });
            continue;
        }
        finger.touches_for(&report, |touch| {
            diagnostics::add_one(Counter::TouchesSent);
            sender.send(touch);
        });
    }
}

/// Puts the panel to sleep, to sleep listening for a double tap, or back to work, as the new listening needs.
async fn switch_panel(
    panel: &mut TouchPanel<I2cDeviceOnBus, Input<'static>, Output<'static>>,
    from: Listening,
    to: Listening,
) -> Result<(), ()> {
    if from != Listening::Everything
        && from
            != (Listening::WhileDark {
                wake_on_touch: wake_on_touch::TAP,
            })
    {
        panel.wake(&mut Delay).await.map_err(|_| ())?;
    }
    match to {
        Listening::WhileDark {
            wake_on_touch: wake_on_touch::OFF,
        } => panel.sleep(&mut Delay).await.map_err(|_| ()),
        Listening::WhileDark {
            wake_on_touch: wake_on_touch::DOUBLE_TAP,
        } => panel.sleep_until_double_tap(&mut Delay).await.map_err(|_| ()),
        _ => Ok(()),
    }
}
