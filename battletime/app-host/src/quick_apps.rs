//! The quick apps: a ring of apps that a swipe left or right on the face steps through, as on wasp-os.
//!
//! The face sits in the ring before the first app, so stepping past the last app comes back to it.
//! The companion writes the ring to `/settings/quick_apps`, one app id a line, and without that
//! file the ring is `DEFAULT_RING`. Apps in the ring that are not installed are skipped.

/// Where the companion keeps the ring.
pub const QUICK_APPS_PATH: &str = "/settings/quick_apps";

/// The ring until the companion writes one.
pub const DEFAULT_RING: &str = "steps\nstopwatch\nheart\nweather\nmusic\n";

/// Which way round the ring a swipe steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Next,
    Previous,
}

/// Where a step round the ring lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination<'ring> {
    Face,
    App(&'ring str),
}

// ============================================================================
// Stepping round the ring
// ============================================================================

/// Where a step from `current` lands, `None` meaning the face; a ring with no installed app stays on the face.
pub fn step<'ring>(
    ring: &'ring str,
    current: Option<&str>,
    step: Step,
    is_installed: impl Fn(&str) -> bool,
) -> Destination<'ring> {
    let apps = || {
        ring.lines()
            .map(str::trim)
            .filter(|id| !id.is_empty() && is_installed(id))
    };
    let count = apps().count();
    // Place 0 is the face and place n is the nth installed app.
    let places = count + 1;
    let place = current
        .and_then(|id| apps().position(|app| app == id))
        .map_or(0, |index| index + 1);
    let next_place = match step {
        Step::Next => (place + 1) % places,
        Step::Previous => (place + places - 1) % places,
    };
    match next_place {
        0 => Destination::Face,
        place => apps().nth(place - 1).map_or(Destination::Face, Destination::App),
    }
}

#[cfg(test)]
#[path = "quick_apps_tests.rs"]
mod quick_apps_tests;
