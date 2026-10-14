//! The way back: every screen the wearer moved on from, so going back retraces them in reverse.
//!
//! Each step forward records the screen left and the transition that left it. Going back takes the
//! newest step and comes in by the opposite transition, so the screens and their movements reverse
//! exactly. A step forward onto the screen just left is a step back instead, so moving away and
//! straight back never piles up. Only the newest `MOST_STEPS` are kept.

use alloc::vec::Vec;

use crate::Transition;

/// How many steps back are remembered; older ones are forgotten.
pub const MOST_STEPS: usize = 8;

/// A screen the wearer moved on from, and how the next one came in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step<Screen> {
    pub screen: Screen,
    pub came_in_by: Transition,
}

impl<Screen> Step<Screen> {
    /// How the screen comes back in: the opposite of the way the next one came in over it.
    pub fn way_back(&self) -> Transition {
        opposite(self.came_in_by)
    }
}

/// The steps taken forward, newest last.
#[derive(Debug)]
pub struct WayBack<Screen> {
    steps: Vec<Step<Screen>>,
}

impl<Screen> Default for WayBack<Screen> {
    fn default() -> Self {
        WayBack { steps: Vec::new() }
    }
}

impl<Screen: Copy + PartialEq> WayBack<Screen> {
    /// Records leaving `from` for `to`; moving onto the screen just left takes that step back instead.
    pub fn forward(&mut self, from: Screen, to: Screen, came_in_by: Transition) {
        if self.steps.last().is_some_and(|step| step.screen == to) {
            self.steps.pop();
            return;
        }
        if self.steps.len() == MOST_STEPS {
            self.steps.remove(0);
        }
        self.steps.push(Step {
            screen: from,
            came_in_by,
        });
    }

    /// The newest screen left, taken off the way back, or none when there is nowhere to go back to.
    pub fn back(&mut self) -> Option<Step<Screen>> {
        self.steps.pop()
    }

    /// Forgets every step, as when the wearer is home on the watch face.
    pub fn clear(&mut self) {
        self.steps.clear();
    }

    /// Forgets every step back to a screen that is gone, such as a temporary app the phone closed.
    pub fn forget(&mut self, is_gone: impl Fn(&Screen) -> bool) {
        self.steps.retain(|step| !is_gone(&step.screen));
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// The transition that undoes another: a slide up is undone by a slide down, a wipe left by a wipe right.
pub fn opposite(transition: Transition) -> Transition {
    match transition {
        Transition::None => Transition::None,
        Transition::Up => Transition::Down,
        Transition::Down => Transition::Up,
        Transition::Left => Transition::Right,
        Transition::Right => Transition::Left,
    }
}

#[cfg(test)]
#[path = "way_back_tests.rs"]
mod way_back_tests;
