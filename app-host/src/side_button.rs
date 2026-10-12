//! What the side button's gestures do in the shell.
//!
//! Any gesture on a dark screen only wakes it. A click or a long press goes to the open app first,
//! which runs its `onButton` or goes back one of its own screens. A click the app leaves goes to
//! the watch face, and a click on the face puts the watch to sleep; on a screen the shell put over
//! another, such as the battery screen, it goes back to that one. A long press the app leaves goes back a
//! step. A double click opens the notifications app, or goes from it to the face. Holding the
//! button for 2 seconds goes to the face whatever the app does.

use neotime_button::Gesture;
use neotime_runtime::events::ButtonPress;

/// The notifications app, which a double click opens, as does a notification that wakes the screen.
pub const NOTIFICATIONS_APP_ID: &str = "notifications";

// ============================================================================
// The shell as the button sees it
// ============================================================================

/// What the shell shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenScreen {
    Face,
    Launcher,
    App,
    NotificationsApp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonSituation {
    pub screen_is_lit: bool,
    pub open_screen: OpenScreen,
    pub notifications_app_is_installed: bool,
    /// Whether the shell opened the app over another that going back returns to, as the battery screen is.
    pub covers_an_earlier_screen: bool,
}

/// What the shell does with a gesture the open app did not take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonAction {
    Nothing,
    WakeScreen,
    PutToSleep,
    GoBack,
    GoToFace,
    OpenNotifications,
}

// ============================================================================
// Gestures to actions
// ============================================================================

/// The press the open app's `onButton` is offered first, or `None` when the shell acts straight away.
pub fn press_for_app(gesture: Gesture, situation: &ButtonSituation) -> Option<ButtonPress> {
    if !situation.screen_is_lit {
        return None;
    }
    match gesture {
        Gesture::Click => Some(ButtonPress::Press),
        Gesture::LongPress => Some(ButtonPress::LongPress),
        Gesture::DoubleClick | Gesture::LongerPress => None,
    }
}

/// What the shell does when the open app was not offered the gesture or did not take it.
pub fn action_for(gesture: Gesture, situation: &ButtonSituation) -> ButtonAction {
    if !situation.screen_is_lit {
        return ButtonAction::WakeScreen;
    }
    let on_face = situation.open_screen == OpenScreen::Face;
    match gesture {
        Gesture::Click if on_face => ButtonAction::PutToSleep,
        Gesture::Click if situation.covers_an_earlier_screen => ButtonAction::GoBack,
        Gesture::Click => ButtonAction::GoToFace,
        Gesture::LongPress if on_face => ButtonAction::Nothing,
        Gesture::LongPress => ButtonAction::GoBack,
        Gesture::DoubleClick if situation.open_screen == OpenScreen::NotificationsApp => {
            ButtonAction::GoToFace
        }
        Gesture::DoubleClick if situation.notifications_app_is_installed => ButtonAction::OpenNotifications,
        Gesture::DoubleClick => ButtonAction::Nothing,
        Gesture::LongerPress if on_face => ButtonAction::Nothing,
        Gesture::LongerPress => ButtonAction::GoToFace,
    }
}

#[cfg(test)]
#[path = "side_button_tests.rs"]
mod side_button_tests;
