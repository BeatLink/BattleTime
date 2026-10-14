use super::*;

const ALL_DIRECTIONS: [SwipeDirection; 4] = [
    SwipeDirection::Up,
    SwipeDirection::Down,
    SwipeDirection::Left,
    SwipeDirection::Right,
];

// ============================================================================
// On the watch face
// ============================================================================

#[test]
fn on_the_face_up_opens_the_launcher_and_down_the_notifications() {
    assert_eq!(
        action_for(SwipeDirection::Up, SwipeScreen::Face),
        SwipeAction::OpenLauncher
    );
    assert_eq!(
        action_for(SwipeDirection::Down, SwipeScreen::Face),
        SwipeAction::OpenNotifications
    );
}

#[test]
fn on_the_face_and_a_quick_app_left_and_right_step_round_the_ring() {
    for screen in [SwipeScreen::Face, SwipeScreen::QuickApp] {
        assert_eq!(
            action_for(SwipeDirection::Left, screen),
            SwipeAction::NextQuickApp
        );
        assert_eq!(
            action_for(SwipeDirection::Right, screen),
            SwipeAction::PreviousQuickApp
        );
    }
}

#[test]
fn a_quick_app_keeps_up_and_down_to_itself() {
    for direction in [SwipeDirection::Up, SwipeDirection::Down] {
        assert_eq!(action_for(direction, SwipeScreen::QuickApp), SwipeAction::Nothing);
    }
}

// ============================================================================
// The pulled-down screens
// ============================================================================

#[test]
fn a_swipe_down_on_the_notifications_opens_quick_settings() {
    assert_eq!(
        action_for(SwipeDirection::Down, SwipeScreen::Notifications),
        SwipeAction::OpenQuickSettings
    );
}

#[test]
fn a_swipe_up_on_the_notifications_or_quick_settings_goes_back() {
    for screen in [SwipeScreen::Notifications, SwipeScreen::QuickSettings] {
        assert_eq!(action_for(SwipeDirection::Up, screen), SwipeAction::GoBack);
    }
}

#[test]
fn the_pulled_down_screens_leave_left_and_right_alone() {
    for screen in [SwipeScreen::Notifications, SwipeScreen::QuickSettings] {
        for direction in [SwipeDirection::Left, SwipeDirection::Right] {
            assert_eq!(action_for(direction, screen), SwipeAction::Nothing);
        }
    }
    assert_eq!(
        action_for(SwipeDirection::Down, SwipeScreen::QuickSettings),
        SwipeAction::Nothing
    );
}

// ============================================================================
// Everywhere else
// ============================================================================

#[test]
fn a_swipe_down_on_the_launcher_goes_back_and_it_keeps_the_rest() {
    assert_eq!(
        action_for(SwipeDirection::Down, SwipeScreen::Launcher),
        SwipeAction::GoBack
    );
    for direction in [SwipeDirection::Up, SwipeDirection::Left, SwipeDirection::Right] {
        assert_eq!(action_for(direction, SwipeScreen::Launcher), SwipeAction::Nothing);
    }
}

#[test]
fn other_apps_keep_every_swipe() {
    for direction in ALL_DIRECTIONS {
        assert_eq!(action_for(direction, SwipeScreen::Other), SwipeAction::Nothing);
    }
}
