//! Decides what the sensor does: what the open app asks for first, then the background interval.
//!
//! An app can have it measure, until stopped or until one reading, or flash the LED as a beacon.
//! With no app asking, the heart rate interval setting measures always, never, or now and then: it
//! measures until a reading or until `BACKGROUND_ATTEMPT_IN_MILLISECONDS` pass, as with the watch
//! off the wrist, then waits the interval before the next.

/// What the open app asks of the sensor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppRequest {
    Nothing,
    Measure,
    Flash { drive: u8, wait: u8 },
}

/// What the sensor should be doing now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Measure,
    /// The LED flashes at this drive level (0 to 3) and wait time (0 the slowest to 7).
    Flash {
        drive: u8,
        wait: u8,
    },
}

/// What the heart rate interval setting asks for with no app asking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    Off,
    Continuous,
    Every { seconds: u32 },
}

/// How long a background measurement tries before it gives up until the next interval.
pub const BACKGROUND_ATTEMPT_IN_MILLISECONDS: u64 = 30_000;

/// The chip's wait times between flashes, in milliseconds, slowest first, as the beacon app names them.
const WAIT_TIMES_IN_MILLISECONDS: [u32; 8] = [800, 400, 200, 100, 75, 50, 12, 0];
const HIGHEST_DRIVE: u8 = 3;

/// The chip's drive level and wait time for a beacon's brightness (0 to 3) and flash period in milliseconds.
pub fn flash_request(brightness: i64, period_in_milliseconds: i64) -> AppRequest {
    let period = period_in_milliseconds.clamp(0, u32::MAX as i64) as u32;
    let wait = WAIT_TIMES_IN_MILLISECONDS
        .iter()
        .position(|&wait_time| wait_time <= period)
        .unwrap_or(WAIT_TIMES_IN_MILLISECONDS.len() - 1) as u8;
    AppRequest::Flash {
        drive: brightness.clamp(0, HIGHEST_DRIVE as i64) as u8,
        wait,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Schedule {
    /// When the next background measurement starts, or `None` before the first.
    next_start_in_milliseconds: Option<u64>,
    /// When the background measurement under way gives up, or `None` while none is.
    gives_up_at_in_milliseconds: Option<u64>,
}

impl Schedule {
    pub const fn new() -> Schedule {
        Schedule {
            next_start_in_milliseconds: None,
            gives_up_at_in_milliseconds: None,
        }
    }

    /// What the sensor should do at `now`, starting or giving up a background measurement as it comes due.
    pub fn mode(&mut self, request: AppRequest, background: Background, now_in_milliseconds: u64) -> Mode {
        match request {
            AppRequest::Flash { drive, wait } => return Mode::Flash { drive, wait },
            AppRequest::Measure => return Mode::Measure,
            AppRequest::Nothing => {}
        }
        let seconds = match background {
            Background::Every { seconds } => u64::from(seconds),
            Background::Continuous | Background::Off => {
                *self = Schedule::new();
                return match background {
                    Background::Continuous => Mode::Measure,
                    _ => Mode::Off,
                };
            }
        };

        if let Some(gives_up_at) = self.gives_up_at_in_milliseconds {
            if now_in_milliseconds < gives_up_at {
                return Mode::Measure;
            }
            self.gives_up_at_in_milliseconds = None;
            self.next_start_in_milliseconds = Some(now_in_milliseconds + seconds * 1000);
        }
        let next_start = *self.next_start_in_milliseconds.get_or_insert(now_in_milliseconds);
        if now_in_milliseconds < next_start {
            return Mode::Off;
        }
        self.gives_up_at_in_milliseconds = Some(now_in_milliseconds + BACKGROUND_ATTEMPT_IN_MILLISECONDS);
        Mode::Measure
    }

    /// Ends a background measurement once it gives a reading, so the next waits the whole interval.
    pub fn reading_taken(&mut self, background: Background, now_in_milliseconds: u64) {
        if let (Some(_), Background::Every { seconds }) = (self.gives_up_at_in_milliseconds, background) {
            self.gives_up_at_in_milliseconds = None;
            self.next_start_in_milliseconds = Some(now_in_milliseconds + u64::from(seconds) * 1000);
        }
    }

    /// When `mode` next needs asking while nothing else happens: the next start or the give-up time.
    pub fn wake_at_in_milliseconds(&self) -> Option<u64> {
        self.gives_up_at_in_milliseconds
            .or(self.next_start_in_milliseconds)
    }
}
