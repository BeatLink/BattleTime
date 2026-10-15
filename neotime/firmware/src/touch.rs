//! The touch panel, turned into the touches the app host takes.

use core::future::pending;

use embassy_futures::select::{select3, Either3};
use embassy_nrf::gpio::{Input, Output};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_graphics::geometry::Point;
use neotime_app_host::Touch;
use neotime_runtime::events::SwipeDirection;
use neotime_settings::wake_on_touch;
use neotime_touch::{
    Finger as PanelFinger, FingerStep, Gesture, GestureFinder, Settings, TouchPanel, TouchReport,
    DRAG_THRESHOLD_IN_PIXELS,
};

type Panel = TouchPanel<I2cDeviceOnBus, Input<'static>, Output<'static>>;

use crate::{
    diagnostics::{self, Counter},
    foreground, motion, I2cDeviceOnBus,
};

/// Touches waiting for the UI, which merges queued drags.
pub static TOUCHES: Channel<CriticalSectionRawMutex, Touch, 5> = Channel::new();

/// Places in the queue drags never take, so a lift's fling, the lift and its gesture always fit.
const PLACES_KEPT_FOR_LIFTS: usize = 3;

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

/// While dark and woken by a tap, the motion task watches for a still wrist, for background heart rate.
const WAITS_FOR_A_TAP: Listening = Listening::WhileDark {
    wake_on_touch: wake_on_touch::TAP,
};

/// Tells the touch task what to listen for; only the newest request counts.
pub fn listen_for(listening: Listening) {
    motion::watch_for_stillness(listening == WAITS_FOR_A_TAP);
    LISTENING.signal(listening);
}

/// Signalled once the panel has started, or given up starting.
pub static STARTED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

const START_ATTEMPTS: usize = 5;
const RETRY_DELAY: Duration = Duration::from_millis(100);

/// The panel reports again and again while a finger stays down, so this long without a report means its lift was lost.
const LOST_LIFT_TIMEOUT: Duration = Duration::from_millis(100);

/// Follows one finger from press to lift, so each touch gives at most one gesture.
#[derive(Default)]
struct Finger {
    panel_finger: PanelFinger,
    gestures: GestureFinder,
    /// Where the finger landed, and once it drags, where it crossed the drag threshold, so dragged content does not jump.
    start: Option<(i32, i32)>,
    dragging: bool,
}

impl Finger {
    fn touches_for(&mut self, report: &TouchReport, now_in_milliseconds: u64, send: impl FnMut(Touch)) {
        let step = self.panel_finger.step(report, now_in_milliseconds);
        let (x, y) = (report.position.column as i32, report.position.row as i32);
        self.take_step(step, x, y, now_in_milliseconds, send);
    }

    /// Ends a touch whose lift the panel never reported, where the finger was last.
    fn lift_lost(&mut self, now_in_milliseconds: u64, send: impl FnMut(Touch)) {
        if let Some(last) = self.panel_finger.lift() {
            self.take_step(FingerStep::Lifts, last.x, last.y, now_in_milliseconds, send);
        }
    }

    fn take_step(
        &mut self,
        step: FingerStep,
        x: i32,
        y: i32,
        now_in_milliseconds: u64,
        mut send: impl FnMut(Touch),
    ) {
        if step == FingerStep::Lands {
            *self = Finger {
                panel_finger: self.panel_finger,
                gestures: self.gestures,
                start: Some((x, y)),
                ..Finger::default()
            };
        }
        let gesture = self.gestures.step(step, Point::new(x, y), now_in_milliseconds);
        match step {
            FingerStep::Lands => {
                foreground::set_finger_down(true);
                send(Touch::Press { x, y });
            }
            FingerStep::Moves => {
                let (start_x, start_y) = *self.start.get_or_insert((x, y));
                let moved = (x - start_x).abs().max((y - start_y).abs());
                if !self.dragging && moved > DRAG_THRESHOLD_IN_PIXELS {
                    self.dragging = true;
                    self.start = Some((
                        start_x + (x - start_x) * DRAG_THRESHOLD_IN_PIXELS / moved,
                        start_y + (y - start_y) * DRAG_THRESHOLD_IN_PIXELS / moved,
                    ));
                }
                if let (true, Some((start_x, start_y))) = (self.dragging, self.start) {
                    send(Touch::Drag {
                        x,
                        y,
                        start_x,
                        start_y,
                    });
                }
            }
            FingerStep::Lifts => {
                foreground::set_finger_down(false);
                if self.dragging {
                    let speed = self.gestures.speed();
                    send(Touch::Fling {
                        x_speed: speed.x,
                        y_speed: speed.y,
                    });
                }
                send(Touch::Release { x, y })
            }
            FingerStep::Nothing => {}
        }

        let gesture = match gesture {
            Gesture::Tap => Touch::Click { x, y },
            Gesture::LongPress => Touch::LongPress { x, y },
            Gesture::SlideUp => Touch::Swipe(SwipeDirection::Up),
            Gesture::SlideDown => Touch::Swipe(SwipeDirection::Down),
            Gesture::SlideLeft => Touch::Swipe(SwipeDirection::Left),
            Gesture::SlideRight => Touch::Swipe(SwipeDirection::Right),
            _ => return,
        };
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
    crate::tasks::name_this_task("touch");
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
    if let Ok(info) = panel.read_chip_info().await {
        crate::about::note_touch_chip(info.chip_id, info.firmware_version);
    }
    let mut finger = Finger::default();
    let mut sender = TouchSender::default();
    let mut listening = Listening::Everything;
    loop {
        let is_finger_down = finger.panel_finger.is_down();
        let lost_lift = async {
            match is_finger_down {
                true => Timer::after(LOST_LIFT_TIMEOUT).await,
                false => pending().await,
            }
        };
        let (report, reported_at) =
            match select3(read_timed_report(&mut panel), LISTENING.wait(), lost_lift).await {
                Either3::First(Ok(timed_report)) => timed_report,
                Either3::First(Err(())) => continue,
                Either3::Second(next) => {
                    if next != listening {
                        let is_awake = listening == Listening::Everything || listening == WAITS_FOR_A_TAP;
                        let _ = switch_panel(&mut panel, is_awake, next).await;
                        listening = next;
                        finger = Finger::default();
                    }
                    continue;
                }
                Either3::Third(()) => {
                    finger.lift_lost(Instant::now().as_millis(), |touch| {
                        diagnostics::add_one(Counter::TouchesSent);
                        sender.send(touch);
                    });
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
        finger.touches_for(&report, reported_at, |touch| {
            diagnostics::add_one(Counter::TouchesSent);
            sender.send(touch);
        });
    }
}

/// Waits for the panel's next report, timed by its interrupt pulse rather than the later read, so speeds come out right.
async fn read_timed_report(panel: &mut Panel) -> Result<(TouchReport, u64), ()> {
    loop {
        panel.wait_for_pulse().await.map_err(|_| ())?;
        let pulsed_at = Instant::now().as_millis();
        if let Some(report) = panel.read_report().await.map_err(|_| ())? {
            return Ok((report, pulsed_at));
        }
    }
}

/// Puts the panel to sleep, to sleep listening for a double tap, or back to work, as the new listening needs.
async fn switch_panel(panel: &mut Panel, is_awake: bool, to: Listening) -> Result<(), ()> {
    if !is_awake {
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
