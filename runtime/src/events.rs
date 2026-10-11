//! The events widgets and screens send to handlers, and the order of each event's fields.
//!
//! A handler reads an event's fields with `ReadEvent`, by position. The compiler turns a field name
//! such as `event.x` into its position from `EventKind::fields`, and the runtime fills the fields
//! in that same order, so this list is the one both sides follow.

use crate::widgets::Property;

// ============================================================================
// Event kinds
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Click,
    LongPress,
    DoubleClick,
    Press,
    Release,
    Drag,
    Swipe,
    Button,
    Change,
    PageChange,
    ScrollEnd,
    Show,
    Hide,
}

impl EventKind {
    /// The event a widget property such as `OnClick` names, if it names one.
    pub fn for_property(property: Property) -> Option<EventKind> {
        use EventKind::*;
        Some(match property {
            Property::OnClick => Click,
            Property::OnLongPress => LongPress,
            Property::OnDoubleClick => DoubleClick,
            Property::OnPress => Press,
            Property::OnRelease => Release,
            Property::OnDrag => Drag,
            Property::OnSwipe => Swipe,
            Property::OnButton => Button,
            Property::OnChange => Change,
            Property::OnPageChange => PageChange,
            Property::OnScrollEnd => ScrollEnd,
            Property::OnShow => Show,
            Property::OnHide => Hide,
            _ => return None,
        })
    }

    /// The event's fields, in the order `ReadEvent` numbers them.
    pub fn fields(self) -> &'static [&'static str] {
        use EventKind::*;
        match self {
            Click | LongPress | DoubleClick | Press | Release => &["x", "y"],
            Drag => &["x", "y", "start_x", "start_y"],
            Swipe => &["direction"],
            Button => &["kind"],
            Change => &["value"],
            PageChange => &["page"],
            ScrollEnd => &["offset"],
            Show | Hide => &[],
        }
    }

    /// The position of a named field, for the compiler.
    pub fn field_index(self, name: &str) -> Option<u8> {
        self.fields()
            .iter()
            .position(|field| *field == name)
            .map(|index| index as u8)
    }
}

// ============================================================================
// Field values
// ============================================================================

/// The `direction` of a swipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
}

/// The `kind` of a side button press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonPress {
    Press,
    LongPress,
}
