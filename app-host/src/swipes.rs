//! What a swipe the open screen does not use does in the shell.
//!
//! Swipes belong to the open app; the shell acts only on the face and the screens reached from it.
//! On the face a swipe up opens the launcher, a swipe down pulls the notifications down, and a
//! swipe left or right steps round the quick apps. On the notifications a swipe down pulls quick
//! settings down after them, and a swipe up on either puts them away to the face. A swipe down on
//! the launcher closes it to the face. On a quick app a swipe left or right steps on round the ring.

use neotime_runtime::events::SwipeDirection;

// ============================================================================
// The shell as a swipe sees it
// ============================================================================

/// What the shell shows, as far as swipes care.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeScreen {
    Face,
    Launcher,
    Notifications,
    QuickSettings,
    /// An app the shell opened from the quick-app ring.
    QuickApp,
    /// Any other app, which keeps every swipe to itself.
    Other,
}

/// What the shell does with a swipe the open screen did not take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeAction {
    Nothing,
    OpenLauncher,
    OpenNotifications,
    OpenQuickSettings,
    NextQuickApp,
    PreviousQuickApp,
    CloseToFace,
}

// ============================================================================
// Swipes to actions
// ============================================================================

pub fn action_for(direction: SwipeDirection, screen: SwipeScreen) -> SwipeAction {
    use SwipeDirection::*;
    match (screen, direction) {
        (SwipeScreen::Face, Up) => SwipeAction::OpenLauncher,
        (SwipeScreen::Face, Down) => SwipeAction::OpenNotifications,
        (SwipeScreen::Face | SwipeScreen::QuickApp, Left) => SwipeAction::NextQuickApp,
        (SwipeScreen::Face | SwipeScreen::QuickApp, Right) => SwipeAction::PreviousQuickApp,
        (SwipeScreen::Notifications, Down) => SwipeAction::OpenQuickSettings,
        (SwipeScreen::Notifications | SwipeScreen::QuickSettings, Up) => SwipeAction::CloseToFace,
        (SwipeScreen::Launcher, Down) => SwipeAction::CloseToFace,
        _ => SwipeAction::Nothing,
    }
}

#[cfg(test)]
#[path = "swipes_tests.rs"]
mod swipes_tests;
