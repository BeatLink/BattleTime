//! What the side button's gestures do in the shell.
//!
//! Any gesture on a dark screen only wakes it. Apps never hear the button. A click means back: the
//! open app first goes back one of its own screens, then the shell goes back a step along the way
//! the wearer came, and on the face a click puts the watch to sleep. A long press, or holding for 2
//! seconds, goes to the face. A double click opens the notifications, or goes from them to the face.
//! On the face chooser a click goes back at once, keeping the face shown, since the face is not asked.

use battletime_button::Gesture;

// ============================================================================
// The shell as the button sees it
// ============================================================================

/// What the shell shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenScreen {
    Face,
    Launcher,
    App,
    Notifications,
    /// A face shown to pick, whose own screens the button never steps back through.
    FaceChooser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonSituation {
    pub screen_is_lit: bool,
    pub open_screen: OpenScreen,
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

/// Whether the open app is asked first to go back one of its own screens.
pub fn goes_back_in_the_app_first(gesture: Gesture, situation: &ButtonSituation) -> bool {
    let shows_an_app = !matches!(situation.open_screen, OpenScreen::Face | OpenScreen::FaceChooser);
    situation.screen_is_lit && gesture == Gesture::Click && shows_an_app
}

/// What the shell does when the open app had no earlier screen to go back to, or was not asked.
pub fn action_for(gesture: Gesture, situation: &ButtonSituation) -> ButtonAction {
    if !situation.screen_is_lit {
        return ButtonAction::WakeScreen;
    }
    let on_face = situation.open_screen == OpenScreen::Face;
    match gesture {
        Gesture::Click if on_face => ButtonAction::PutToSleep,
        Gesture::Click => ButtonAction::GoBack,
        Gesture::LongPress | Gesture::LongerPress if on_face => ButtonAction::Nothing,
        Gesture::LongPress | Gesture::LongerPress => ButtonAction::GoToFace,
        Gesture::DoubleClick if situation.open_screen == OpenScreen::Notifications => ButtonAction::GoToFace,
        Gesture::DoubleClick => ButtonAction::OpenNotifications,
    }
}

#[cfg(test)]
#[path = "side_button_tests.rs"]
mod side_button_tests;
