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
    OpenScreen::NotificationsApp,
];

fn lit_on(open_screen: OpenScreen) -> ButtonSituation {
    ButtonSituation {
        screen_is_lit: true,
        open_screen,
        notifications_app_is_installed: true,
        covers_an_earlier_screen: false,
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
            assert_eq!(press_for_app(gesture, &situation), None);
            assert_eq!(action_for(gesture, &situation), ButtonAction::WakeScreen);
        }
    }
}

// ============================================================================
// Clicks
// ============================================================================

#[test]
fn a_click_is_offered_to_the_open_app_as_a_press() {
    for open_screen in [
        OpenScreen::Face,
        OpenScreen::Launcher,
        OpenScreen::App,
        OpenScreen::NotificationsApp,
    ] {
        assert_eq!(
            press_for_app(Gesture::Click, &lit_on(open_screen)),
            Some(ButtonPress::Press)
        );
    }
}

#[test]
fn a_click_the_app_leaves_goes_home() {
    for open_screen in [
        OpenScreen::Launcher,
        OpenScreen::App,
        OpenScreen::NotificationsApp,
    ] {
        assert_eq!(
            action_for(Gesture::Click, &lit_on(open_screen)),
            ButtonAction::GoToFace
        );
    }
}

#[test]
fn a_click_on_a_screen_over_another_goes_back_to_it() {
    let situation = ButtonSituation {
        covers_an_earlier_screen: true,
        ..lit_on(OpenScreen::App)
    };

    assert_eq!(
        press_for_app(Gesture::Click, &situation),
        Some(ButtonPress::Press)
    );
    assert_eq!(action_for(Gesture::Click, &situation), ButtonAction::GoBack);
}

#[test]
fn a_click_the_face_leaves_puts_the_watch_to_sleep() {
    let situation = lit_on(OpenScreen::Face);

    assert_eq!(action_for(Gesture::Click, &situation), ButtonAction::PutToSleep);
}

// ============================================================================
// Long presses
// ============================================================================

#[test]
fn a_long_press_is_offered_to_the_open_app_and_otherwise_goes_back() {
    let situation = lit_on(OpenScreen::App);

    assert_eq!(
        press_for_app(Gesture::LongPress, &situation),
        Some(ButtonPress::LongPress)
    );
    assert_eq!(action_for(Gesture::LongPress, &situation), ButtonAction::GoBack);
}

#[test]
fn a_long_press_the_face_leaves_does_nothing() {
    let situation = lit_on(OpenScreen::Face);

    assert_eq!(action_for(Gesture::LongPress, &situation), ButtonAction::Nothing);
}

#[test]
fn holding_for_two_seconds_goes_to_the_face_whatever_the_app_does() {
    for open_screen in [
        OpenScreen::Launcher,
        OpenScreen::App,
        OpenScreen::NotificationsApp,
    ] {
        let situation = lit_on(open_screen);

        assert_eq!(press_for_app(Gesture::LongerPress, &situation), None);
        assert_eq!(
            action_for(Gesture::LongerPress, &situation),
            ButtonAction::GoToFace
        );
    }
    assert_eq!(
        action_for(Gesture::LongerPress, &lit_on(OpenScreen::Face)),
        ButtonAction::Nothing
    );
}

// ============================================================================
// Double clicks
// ============================================================================

#[test]
fn a_double_click_opens_the_notifications_from_anywhere_else() {
    for open_screen in [OpenScreen::Face, OpenScreen::Launcher, OpenScreen::App] {
        let situation = lit_on(open_screen);

        assert_eq!(press_for_app(Gesture::DoubleClick, &situation), None);
        assert_eq!(
            action_for(Gesture::DoubleClick, &situation),
            ButtonAction::OpenNotifications
        );
    }
}

#[test]
fn a_double_click_in_the_notifications_goes_to_the_face() {
    assert_eq!(
        action_for(Gesture::DoubleClick, &lit_on(OpenScreen::NotificationsApp)),
        ButtonAction::GoToFace
    );
}

#[test]
fn a_double_click_does_nothing_without_a_notifications_app() {
    let situation = ButtonSituation {
        notifications_app_is_installed: false,
        ..lit_on(OpenScreen::Face)
    };

    assert_eq!(
        action_for(Gesture::DoubleClick, &situation),
        ButtonAction::Nothing
    );
}
