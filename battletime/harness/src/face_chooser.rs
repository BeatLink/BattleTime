//! The face chooser, as the watch's shell shows it when an app calls `choose_watch_face`.
//!
//! The shown face opens full screen in place of the app and hears none of its touches. A swipe
//! left or right, as `battletime_app_host::swipes` says, saves the next or previous face turned on
//! in `settings.watch_face` and opens it, and the side button goes back to the app that asked.

use battletime_app_host::side_button::{self, ButtonAction, ButtonSituation, OpenScreen};
use battletime_app_host::swipes::{self, SwipeAction, SwipeScreen};
use battletime_app_host::{EventResult, Touch};
use battletime_button::Gesture;
use battletime_packages::faces::{face_number, face_to_show, stepped_face};
use battletime_runtime::{SystemValue, SystemValues};

use super::TestWatch;
use crate::sources::OpenFailure;

// ============================================================================
// Opening and leaving
// ============================================================================

impl TestWatch {
    /// Whether the face chooser shows, with a face in place of the app that opened it.
    pub fn is_choosing_face(&self) -> bool {
        self.face_chooser_returns_to.is_some()
    }

    /// Opens the face chooser on the face shown when the open app asked for it, as the shell does once the handler returns.
    pub(super) fn open_face_chooser_if_asked(&mut self) {
        if !std::mem::take(&mut self.system.face_chooser_wanted) || self.is_choosing_face() {
            return;
        }
        let Some(face) = self.shown_face() else {
            return;
        };
        let app = self.id().to_string();
        self.switch_to_face(&face)
            .unwrap_or_else(|failure| panic!("the face chooser's face {face} would not open: {failure}"));
        self.face_chooser_returns_to = Some(app);
    }

    /// Takes a touch on the face chooser: a swipe left or right steps through the faces and nothing reaches the face.
    pub(super) fn face_chooser_touch(&mut self, touch: Touch) -> EventResult {
        let Touch::Swipe(direction) = touch else {
            return EventResult::NotHandled;
        };
        let steps = match swipes::action_for(direction, SwipeScreen::FaceChooser) {
            SwipeAction::NextFace => 1,
            SwipeAction::PreviousFace => -1,
            _ => return EventResult::NotHandled,
        };
        self.step_face(steps)
            .unwrap_or_else(|failure| panic!("the next face would not open: {failure}"));
        EventResult::Handled
    }

    /// Takes a click of the side button on the face chooser, which goes back to the app keeping the face chosen.
    pub(super) fn face_chooser_button(&mut self) -> EventResult {
        let situation = ButtonSituation {
            screen_is_lit: true,
            open_screen: OpenScreen::FaceChooser,
        };
        if side_button::action_for(Gesture::Click, &situation) != ButtonAction::GoBack {
            return EventResult::NotHandled;
        }
        let Some(app) = self.face_chooser_returns_to.take() else {
            return EventResult::NotHandled;
        };
        self.switch_to_app(&app)
            .unwrap_or_else(|failure| panic!("{app} would not open again: {failure}"));
        EventResult::Handled
    }
}

// ============================================================================
// The faces turned on
// ============================================================================

impl TestWatch {
    /// The ids of the faces turned on, in id order, as the shell keeps them.
    fn face_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = (self.system.packages.iter())
            .filter(|package| package.is_face && package.enabled)
            .map(|package| package.id.clone())
            .collect();
        ids.sort();
        ids
    }

    /// The face `settings.watch_face` chooses, or the first turned on when that one is off or gone.
    fn shown_face(&self) -> Option<String> {
        let ids = self.face_ids();
        let chosen = self.system.read(SystemValue::WatchFace) as i32;
        face_to_show(ids.iter().map(String::as_str), chosen).map(String::from)
    }

    /// Saves the face `steps` places on as the choice and opens it in the chooser; with one face nothing changes.
    fn step_face(&mut self, steps: i32) -> Result<(), OpenFailure> {
        let ids = self.face_ids();
        let shown = self.id().to_string();
        let Some(next) = stepped_face(ids.iter().map(String::as_str), &shown, steps).map(String::from) else {
            return Ok(());
        };
        if next == shown {
            return Ok(());
        }
        let name = (self.system.packages.iter())
            .find(|package| package.id == next)
            .map(|package| package.name.clone())
            .unwrap_or_default();
        self.system.set(
            SystemValue::WatchFace,
            face_number(&next) as battletime_runtime::Value,
        );
        self.system.set_text(SystemValue::WatchFaceName, &name);
        self.switch_to_face(&next)
    }
}
