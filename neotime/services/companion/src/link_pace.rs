//! How fast the watch asks the phone to run the Bluetooth link: slowly while idle, to save power,
//! and fast while a file transfer, an update or a packages install is under way.
//!
//! It only chooses; the firmware sends the request as an L2CAP Connection Parameter Update Request,
//! and the phone may turn it down, which changes nothing here.

// ============================================================================
// The parameters
// ============================================================================

/// One set of connection parameters, as the request carries them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkParameters {
    /// The shortest connection interval, in 1.25 ms steps.
    pub interval_min: u16,
    /// The longest connection interval, in 1.25 ms steps.
    pub interval_max: u16,
    /// How many connection events the watch may skip when it has nothing to send.
    pub latency: u16,
    /// How long a silent link lasts before it counts as lost, in 10 ms steps.
    pub supervision_timeout: u16,
}

/// While idle: 200 to 400 ms, two events skipped, six seconds to drop.
pub const IDLE_PARAMETERS: LinkParameters = LinkParameters {
    interval_min: 160,
    interval_max: 320,
    latency: 2,
    supervision_timeout: 600,
};

/// During a transfer: 15 to 30 ms, nothing skipped, six seconds to drop.
pub const BUSY_PARAMETERS: LinkParameters = LinkParameters {
    interval_min: 12,
    interval_max: 24,
    latency: 0,
    supervision_timeout: 600,
};

/// Whether the link outlasts the longest gap the parameters allow, twice over, as the specification asks.
const fn keeps_link_alive(parameters: LinkParameters) -> bool {
    let longest_gap_milliseconds = (parameters.latency as u32 + 1) * parameters.interval_max as u32 * 5 / 4;
    parameters.interval_min <= parameters.interval_max
        && parameters.supervision_timeout as u32 * 10 > 2 * longest_gap_milliseconds
}

const _: () = assert!(keeps_link_alive(IDLE_PARAMETERS) && keeps_link_alive(BUSY_PARAMETERS));

/// How long the link stays fast after the last transfer, and after connecting before it slows.
pub const SETTLE_SECONDS: u64 = 5;

// ============================================================================
// The choice
// ============================================================================

/// Which set the watch last asked for and when the link may go idle, for one connection.
#[derive(Clone, Copy, Debug)]
pub struct LinkPace {
    idle_from_seconds: u64,
    busy: bool,
    asked_busy: Option<bool>,
}

impl LinkPace {
    /// A new connection, made at `now_seconds`, which asks for nothing until it settles.
    pub const fn new(now_seconds: u64) -> Self {
        Self {
            idle_from_seconds: now_seconds + SETTLE_SECONDS,
            busy: false,
            asked_busy: None,
        }
    }

    /// Notes a file transfer, update or packages step, which keeps the link fast for a while.
    pub fn note_transfer(&mut self, now_seconds: u64) {
        self.busy = true;
        self.idle_from_seconds = now_seconds + SETTLE_SECONDS;
    }

    /// The parameters to ask for now, or `None` when the last request already asked for them.
    pub fn request(&mut self, now_seconds: u64) -> Option<LinkParameters> {
        if now_seconds >= self.idle_from_seconds {
            self.busy = false;
        } else if !self.busy {
            return None;
        }
        if self.asked_busy == Some(self.busy) {
            return None;
        }
        self.asked_busy = Some(self.busy);
        Some(if self.busy {
            BUSY_PARAMETERS
        } else {
            IDLE_PARAMETERS
        })
    }

    /// Takes back the last request, which never went out, so it is asked again a second later at the soonest.
    pub fn retry_later(&mut self, now_seconds: u64) {
        self.asked_busy = None;
        self.idle_from_seconds = self.idle_from_seconds.max(now_seconds + 1);
    }

    /// When `request` next has something to ask, or `None` while the link is already idle.
    pub fn next_change_seconds(&self) -> Option<u64> {
        (self.asked_busy != Some(false)).then_some(self.idle_from_seconds)
    }
}
