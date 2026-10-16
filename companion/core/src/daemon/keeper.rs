// When the daemon reconnects to the watch on its own, and how long it waits between tries.

use std::time::Duration;

use super::settings::CoreSettings;

// The wait before the first try after the link drops, doubled after each failure up to the longest.
pub const FIRST_RETRY: Duration = Duration::from_secs(2);
pub const SECOND_RETRY: Duration = Duration::from_secs(5);
pub const LONGEST_RETRY: Duration = Duration::from_secs(5 * 60);

pub fn retry_delay(failures: u32) -> Duration {
    if failures == 0 {
        return FIRST_RETRY;
    }
    let doubled = SECOND_RETRY.saturating_mul(1 << failures.saturating_sub(1).min(16));
    doubled.min(LONGEST_RETRY)
}

// What holds the keeper back besides the settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeeperHolds {
    // The user disconnected, so the watch stays away until they connect again.
    pub user_disconnected: bool,
    // A firmware update owns the link.
    pub held: bool,
    // The window is scanning, which connecting would stop.
    pub scanning: bool,
}

// The watch the daemon should be keeping, whether or not it is connected now.
pub fn kept_watch(settings: &CoreSettings, holds: &KeeperHolds) -> Option<String> {
    if !settings.auto_reconnect || holds.user_disconnected {
        return None;
    }
    settings.last_watch_id.clone().filter(|id| !id.is_empty())
}

// The watch to try now: kept, not held back, and not already connected or connecting.
pub fn watch_to_try(settings: &CoreSettings, holds: &KeeperHolds, disconnected: bool) -> Option<String> {
    if holds.held || holds.scanning || !disconnected {
        return None;
    }
    kept_watch(settings, holds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remembering(id: &str) -> CoreSettings {
        CoreSettings {
            last_watch_id: Some(id.into()),
            ..CoreSettings::default()
        }
    }

    #[test]
    fn the_wait_doubles_after_each_failure_up_to_five_minutes() {
        assert_eq!(retry_delay(0), Duration::from_secs(2));
        assert_eq!(retry_delay(1), Duration::from_secs(5));
        assert_eq!(retry_delay(2), Duration::from_secs(10));
        assert_eq!(retry_delay(3), Duration::from_secs(20));
        assert_eq!(retry_delay(7), Duration::from_secs(300));
        assert_eq!(retry_delay(40), Duration::from_secs(300));
    }

    #[test]
    fn the_remembered_watch_is_tried_only_while_it_is_away() {
        let settings = remembering("AA");
        let none = KeeperHolds::default();

        assert_eq!(watch_to_try(&settings, &none, true), Some("AA".into()));
        assert_eq!(watch_to_try(&settings, &none, false), None);
        assert_eq!(watch_to_try(&CoreSettings::default(), &none, true), None);
        assert_eq!(watch_to_try(&remembering(""), &none, true), None);
    }

    #[test]
    fn turning_reconnect_off_disconnecting_updating_or_scanning_holds_it_back() {
        let settings = remembering("AA");
        let off = CoreSettings {
            auto_reconnect: false,
            ..remembering("AA")
        };

        assert_eq!(watch_to_try(&off, &KeeperHolds::default(), true), None);
        assert_eq!(
            watch_to_try(
                &settings,
                &KeeperHolds {
                    user_disconnected: true,
                    ..KeeperHolds::default()
                },
                true
            ),
            None
        );
        assert_eq!(
            watch_to_try(
                &settings,
                &KeeperHolds {
                    held: true,
                    ..KeeperHolds::default()
                },
                true
            ),
            None
        );
        assert_eq!(
            watch_to_try(
                &settings,
                &KeeperHolds {
                    scanning: true,
                    ..KeeperHolds::default()
                },
                true
            ),
            None
        );
        assert_eq!(
            kept_watch(
                &settings,
                &KeeperHolds {
                    scanning: true,
                    ..KeeperHolds::default()
                }
            ),
            Some("AA".into())
        );
    }
}
