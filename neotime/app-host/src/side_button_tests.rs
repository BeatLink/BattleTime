use super::*;

const ALL_GESTURES: [Gesture; 4] = [
    Gesture::Click,
    Gesture::DoubleClick,
    Gesture::LongPress,
    Gesture::LongerPress,
];

const ALL_SCREENS: [OpenScreen; 4] = [
    OpenScreen::Face,
    OpenScreen::Launcher,
    OpenScreen::App,
    OpenScreen::Notifications,
];

const OFF_THE_FACE: [OpenScreen; 3] = [OpenScreen::Launcher, OpenScreen::App, OpenScreen::Notifications];

fn lit_on(open_screen: OpenScreen) -> ButtonSituation {
    ButtonSituation {
        screen_is_lit: true,
        open_screen,
    }
}

// ============================================================================
// A dark screen
// ============================================================================

#[test]
fn every_gesture_on_a_dark_screen_only_wakes_it() {
    for open_screen in ALL_SCREENS {
        let situation = ButtonSituation {
            screen_is_lit: false,
            ..lit_on(open_screen)
        };

        for gesture in ALL_GESTURES {
            assert!(!goes_back_in_the_app_first(gesture, &situation));
            assert_eq!(action_for(gesture, &situation), ButtonAction::WakeScreen);
        }
    }
}

// ============================================================================
// Clicks
// ============================================================================

#[test]
fn a_click_goes_back_in_the_app_and_then_a_step_in_the_shell() {
    for open_screen in OFF_THE_FACE {
        let situation = lit_on(open_screen);

        assert!(goes_back_in_the_app_first(Gesture::Click, &situation));
        assert_eq!(action_for(Gesture::Click, &situation), ButtonAction::GoBack);
    }
}

#[test]
fn a_click_on_the_face_puts_the_watch_to_sleep() {
    let situation = lit_on(OpenScreen::Face);

    assert!(!goes_back_in_the_app_first(Gesture::Click, &situation));
    assert_eq!(action_for(Gesture::Click, &situation), ButtonAction::PutToSleep);
}

#[test]
fn only_a_click_goes_to_the_app() {
    for open_screen in ALL_SCREENS {
        for gesture in [Gesture::DoubleClick, Gesture::LongPress, Gesture::LongerPress] {
            assert!(!goes_back_in_the_app_first(gesture, &lit_on(open_screen)));
        }
    }
}

// ============================================================================
// Long presses
// ============================================================================

#[test]
fn a_long_press_or_a_two_second_hold_goes_to_the_face() {
    for open_screen in OFF_THE_FACE {
        for gesture in [Gesture::LongPress, Gesture::LongerPress] {
            assert_eq!(action_for(gesture, &lit_on(open_screen)), ButtonAction::GoToFace);
        }
    }
}

#[test]
fn a_long_press_on_the_face_does_nothing() {
    for gesture in [Gesture::LongPress, Gesture::LongerPress] {
        assert_eq!(
            action_for(gesture, &lit_on(OpenScreen::Face)),
            ButtonAction::Nothing
        );
    }
}

// ============================================================================
// Double clicks
// ============================================================================

#[test]
fn a_double_click_opens_the_notifications_from_anywhere_else() {
    for open_screen in [OpenScreen::Face, OpenScreen::Launcher, OpenScreen::App] {
        assert_eq!(
            action_for(Gesture::DoubleClick, &lit_on(open_screen)),
            ButtonAction::OpenNotifications
        );
    }
}

#[test]
fn a_double_click_in_the_notifications_goes_to_the_face() {
    assert_eq!(
        action_for(Gesture::DoubleClick, &lit_on(OpenScreen::Notifications)),
        ButtonAction::GoToFace
    );
}
