use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Face,
    Notifications,
    QuickSettings,
    Launcher,
    App(u8),
}

use Screen::*;

/// Goes back until there is nothing left, giving each screen and how it came back in.
fn all_the_way_back(way_back: &mut WayBack<Screen>) -> alloc::vec::Vec<(Screen, Transition)> {
    core::iter::from_fn(|| way_back.back().map(|step| (step.screen, step.way_back()))).collect()
}

#[test]
fn going_back_retraces_every_screen_in_reverse_with_the_opposite_movements() {
    let mut way_back = WayBack::default();
    way_back.forward(Face, Notifications, Transition::Down);
    way_back.forward(Notifications, QuickSettings, Transition::Down);

    assert_eq!(
        all_the_way_back(&mut way_back),
        [(Notifications, Transition::Up), (Face, Transition::Up)]
    );
}

#[test]
fn a_longer_path_comes_back_through_each_screen() {
    let mut way_back = WayBack::default();
    way_back.forward(Face, Launcher, Transition::Up);
    way_back.forward(Launcher, App(1), Transition::Up);
    way_back.forward(App(1), App(2), Transition::Up);

    assert_eq!(
        all_the_way_back(&mut way_back),
        [
            (App(1), Transition::Down),
            (Launcher, Transition::Down),
            (Face, Transition::Down)
        ]
    );
}

#[test]
fn moving_onto_the_screen_just_left_is_a_step_back() {
    let mut way_back = WayBack::default();
    way_back.forward(Face, App(1), Transition::Left);
    way_back.forward(App(1), App(2), Transition::Left);

    // A swipe the other way returns to the first app rather than adding a third step.
    way_back.forward(App(2), App(1), Transition::Right);

    assert_eq!(all_the_way_back(&mut way_back), [(Face, Transition::Right)]);
}

#[test]
fn only_the_newest_steps_are_kept() {
    let mut way_back = WayBack::default();
    for place in 0..MOST_STEPS as u8 + 2 {
        way_back.forward(App(place), App(place + 1), Transition::Up);
    }

    let screens: alloc::vec::Vec<Screen> = all_the_way_back(&mut way_back)
        .into_iter()
        .map(|(screen, _)| screen)
        .collect();

    assert_eq!(screens.len(), MOST_STEPS);
    assert_eq!(screens.first(), Some(&App(MOST_STEPS as u8 + 1)));
    assert_eq!(screens.last(), Some(&App(2)));
}

#[test]
fn clearing_leaves_nowhere_to_go_back_to() {
    let mut way_back = WayBack::default();
    way_back.forward(Face, Notifications, Transition::Down);
    assert!(!way_back.is_empty());

    way_back.clear();

    assert!(way_back.is_empty());
    assert_eq!(way_back.back(), None);
}

#[test]
fn each_transition_has_its_opposite() {
    let pairs = [
        (Transition::None, Transition::None),
        (Transition::Up, Transition::Down),
        (Transition::Down, Transition::Up),
        (Transition::Left, Transition::Right),
        (Transition::Right, Transition::Left),
    ];
    for (transition, undone_by) in pairs {
        assert_eq!(opposite(transition), undone_by);
    }
}
