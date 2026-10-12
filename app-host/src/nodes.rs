//! One node per widget of the open screen: the values its properties hold and where layout put it.
//!
//! A node keeps only values. The rlvgl widget that draws it is built from them each time it is
//! drawn and dropped after, so a screen costs little UI heap between redraws. Colours are kept as
//! the app gave them, fixed or a theme reference, and resolved only when drawn, so a new theme
//! needs no app code to run.

use alloc::{boxed::Box, string::String};

use neotime_runtime::{widgets::Property, ColorValue, Theme, ThemeColor, Value, WidgetKind};
use rlvgl_core::widget::{Color, Rect};

use crate::fonts::FontName;

// ============================================================================
// Property values
// ============================================================================

/// A length: whole pixels, or a share of the parent's inner size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Length {
    Pixels(i16),
    Percent(i16),
}

impl Length {
    /// A negative value is a percentage, so `-100` means `"100%"`.
    pub fn from_value(value: i32) -> Length {
        match value {
            value if value < 0 => Length::Percent(to_i16(value.saturating_neg())),
            value => Length::Pixels(to_i16(value)),
        }
    }

    pub fn of(self, whole: i32) -> i32 {
        match self {
            Length::Pixels(pixels) => pixels.into(),
            Length::Percent(percent) => whole.saturating_mul(percent.into()) / 100,
        }
    }
}

/// Where a widget sits across its parent's flow, and where a label puts its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

impl Align {
    pub fn from_value(value: i32) -> Option<Align> {
        match value {
            0 => Some(Align::Start),
            1 => Some(Align::Center),
            2 => Some(Align::End),
            _ => None,
        }
    }

    /// How far into `space` a thing of `size` starts.
    pub fn offset(self, space: i32, size: i32) -> i32 {
        match self {
            Align::Start => 0,
            Align::Center => (space - size) / 2,
            Align::End => space - size,
        }
    }
}

/// Where a container puts its children along its flow, or across it for `align_items`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Start,
    Center,
    End,
    SpaceBetween,
    Stretch,
}

impl Placement {
    pub fn from_value(value: i32) -> Option<Placement> {
        match value {
            0 => Some(Placement::Start),
            1 => Some(Placement::Center),
            2 => Some(Placement::End),
            3 => Some(Placement::SpaceBetween),
            4 => Some(Placement::Stretch),
            _ => None,
        }
    }
}

/// What a label does with text wider than its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LongMode {
    Wrap,
    Scroll,
    Clip,
}

impl LongMode {
    pub fn from_value(value: i32) -> LongMode {
        match value {
            1 => LongMode::Scroll,
            2 => LongMode::Clip,
            _ => LongMode::Wrap,
        }
    }
}

/// A property's value once read: a number, or text for `text`.
pub enum PropertyValue<'text> {
    Number(i64),
    Text(&'text str),
}

/// What the common properties of a widget hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub x: Option<i16>,
    pub y: Option<i16>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub grow: i16,
    pub padding: i16,
    pub margin: i16,
    pub gap: i16,
    pub background: Option<ColorValue>,
    pub color: Option<ColorValue>,
    pub border_color: Option<ColorValue>,
    pub border_width: Option<i16>,
    pub radius: Option<i16>,
    pub font: FontName,
    pub align: Option<Align>,
    pub justify: Placement,
    pub align_items: Option<Placement>,
    pub long_mode: LongMode,
    pub visible: bool,
    pub disabled: bool,
    pub opacity: u8,
    pub value: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub start_angle: Option<i16>,
    pub end_angle: Option<i16>,
    /// A Grid's or ButtonMatrix's columns, or a CellGrid's.
    pub columns: u16,
    /// The state array the widget's main array property names: cells, options, points, series or labels.
    pub array: Option<u8>,
}

const DEFAULT_MAXIMUM: i32 = 100;
pub const OPAQUE: u8 = 255;

impl Default for Settings {
    fn default() -> Self {
        Settings {
            x: None,
            y: None,
            width: None,
            height: None,
            grow: 0,
            padding: 0,
            margin: 0,
            gap: 0,
            background: None,
            color: None,
            border_color: None,
            border_width: None,
            radius: None,
            font: FontName::Normal,
            align: None,
            justify: Placement::Start,
            align_items: None,
            long_mode: LongMode::Wrap,
            visible: true,
            disabled: false,
            opacity: OPAQUE,
            value: 0,
            minimum: 0,
            maximum: DEFAULT_MAXIMUM,
            start_angle: None,
            end_angle: None,
            columns: 0,
            array: None,
        }
    }
}

/// The properties only some kinds of widget have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extra {
    None,
    CellGrid {
        rows: u16,
        cell_size: u16,
        cell_gap: u16,
        cell_shape: u8,
        palette: Option<u8>,
    },
    Scale {
        ticks: u16,
        major_every: u16,
        skip: Option<u8>,
        labels: Option<u8>,
        needle_color: Option<ColorValue>,
        major_color: Option<ColorValue>,
    },
    AnalogClock {
        time: Value,
        show_seconds: bool,
        hand_style: u8,
        hour_color: Option<ColorValue>,
        minute_color: Option<ColorValue>,
        second_color: Option<ColorValue>,
        hour_length: Option<u16>,
        minute_length: Option<u16>,
        second_length: Option<u16>,
    },
    Image {
        reference: Option<i32>,
        recolor: Option<ColorValue>,
    },
    /// Which rows of a Repeat have nodes, when its rows have one fixed height and so sit in slots.
    Repeat {
        /// The rows' height, or 0 when every shown row is built.
        slot_extent: i16,
        /// How many rows are shown, each taking a slot.
        slot_count: u16,
        first_built_slot: u16,
        built_slots: u16,
    },
}

impl Extra {
    pub fn for_kind(kind: WidgetKind) -> Extra {
        match kind {
            WidgetKind::CellGrid => Extra::CellGrid {
                rows: 0,
                cell_size: 0,
                cell_gap: 0,
                cell_shape: 0,
                palette: None,
            },
            WidgetKind::Scale => Extra::Scale {
                ticks: 0,
                major_every: 0,
                skip: None,
                labels: None,
                needle_color: None,
                major_color: None,
            },
            WidgetKind::AnalogClock => Extra::AnalogClock {
                time: 0,
                show_seconds: false,
                hand_style: 0,
                hour_color: None,
                minute_color: None,
                second_color: None,
                hour_length: None,
                minute_length: None,
                second_length: None,
            },
            WidgetKind::Repeat => Extra::Repeat {
                slot_extent: 0,
                slot_count: 0,
                first_built_slot: 0,
                built_slots: 0,
            },
            WidgetKind::Image | WidgetKind::Icon | WidgetKind::Button | WidgetKind::ImageButton => {
                Extra::Image {
                    reference: None,
                    recolor: None,
                }
            }
            _ => Extra::None,
        }
    }
}

/// Clamps a 64-bit app value into the 32 bits a widget holds.
fn to_i32(value: i64) -> i32 {
    value.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// Clamps a value into the 16 bits a position, a length or an angle is kept in, which fits any screen.
fn to_i16(value: impl Into<i64>) -> i16 {
    value.into().clamp(i16::MIN as i64, i16::MAX as i64) as i16
}

fn to_u16(value: i64) -> u16 {
    value.clamp(0, u16::MAX as i64) as u16
}

/// An array property's state variable, from the constant the image gives.
fn to_variable(value: i64) -> Option<u8> {
    u8::try_from(value).ok()
}

/// Stores `new` in `place` and says whether that changed it.
fn replace<Value: PartialEq>(place: &mut Value, new: Value) -> bool {
    let changed = *place != new;
    *place = new;
    changed
}

// ============================================================================
// Colours
// ============================================================================

pub const TRANSPARENT: Color = Color(0, 0, 0, 0);

/// Spreads an RGB565 colour to rlvgl's eight bits a channel.
pub fn to_color(rgb565: u16) -> Color {
    let red = (rgb565 >> 11) & 0x1F;
    let green = (rgb565 >> 5) & 0x3F;
    let blue = rgb565 & 0x1F;
    Color(
        ((red << 3) | (red >> 2)) as u8,
        ((green << 2) | (green >> 4)) as u8,
        ((blue << 3) | (blue >> 2)) as u8,
        OPAQUE,
    )
}

pub fn resolve(color: Option<ColorValue>, fallback: ThemeColor, theme: &Theme) -> Color {
    to_color(color.unwrap_or(ColorValue::Theme(fallback)).resolve(theme))
}

pub fn resolve_or_clear(color: Option<ColorValue>, theme: &Theme) -> Color {
    color.map_or(TRANSPARENT, |color| to_color(color.resolve(theme)))
}

/// A widget's `Extra`, kept on the heap only for the few kinds that have one, so every other node stays small.
#[derive(Debug, PartialEq, Eq)]
pub struct ExtraSlot(Option<Box<Extra>>);

impl ExtraSlot {
    fn for_kind(kind: WidgetKind) -> ExtraSlot {
        match Extra::for_kind(kind) {
            Extra::None => ExtraSlot(None),
            extra => ExtraSlot(Some(Box::new(extra))),
        }
    }

    pub fn get_mut(&mut self) -> Option<&mut Extra> {
        self.0.as_deref_mut()
    }
}

impl core::ops::Deref for ExtraSlot {
    type Target = Extra;

    fn deref(&self) -> &Extra {
        self.0.as_deref().unwrap_or(&Extra::None)
    }
}

// ============================================================================
// Nodes
// ============================================================================

pub struct Node {
    /// The widget's number in the image.
    pub widget_index: u16,
    /// Which row of a repeated list the node draws, or 0 outside one.
    pub row: u16,
    pub kind: WidgetKind,
    /// The node just past this one and everything inside it.
    pub subtree_end: u8,
    pub parent: Option<u8>,
    pub settings: Settings,
    pub extra: ExtraSlot,
    pub text: String,
    /// The size the widget's content wants, worked out by the host from images and arrays.
    pub content_width: i16,
    pub content_height: i16,
    pub bounds: Rect,
    /// The part of the screen the widget may draw on: its parents' boxes where they clip, such as a Scroll.
    pub clip: Rect,
    /// Whether the widget and every widget around it is visible.
    pub shown: bool,
    /// How far a Scroll or List has been scrolled down.
    pub scroll_offset: i16,
    /// Whether a Dropdown has its list open.
    pub is_open: bool,
    /// How many options a Dropdown offers, for the height of its open list.
    pub option_count: u8,
    /// Where the widget was last drawn, so moving or hiding it can clear that area.
    pub drawn_bounds: Option<Rect>,
}

impl Node {
    pub fn new(
        widget_index: u16,
        kind: WidgetKind,
        subtree_end: u8,
        parent: Option<u8>,
        screen: Rect,
    ) -> Node {
        Node {
            widget_index,
            row: 0,
            kind,
            subtree_end,
            parent,
            settings: Settings::default(),
            extra: ExtraSlot::for_kind(kind),
            text: String::new(),
            content_width: 0,
            content_height: 0,
            bounds: screen,
            clip: screen,
            shown: false,
            scroll_offset: 0,
            is_open: false,
            option_count: 0,
            drawn_bounds: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The area the widget covers on screen, which for an open Dropdown includes its list.
    pub fn covered_area(&self) -> Option<Rect> {
        if !self.shown {
            return None;
        }
        let area = match self.is_open {
            true => crate::drawing::open_list_area(self),
            false => self.bounds,
        };
        area.intersect(self.clip)
    }

    /// Stores one property's value and says whether anything visible changed.
    pub fn apply(&mut self, property: Property, value: PropertyValue) -> bool {
        let number = match value {
            PropertyValue::Text(text) => {
                if property != Property::Text || self.text == text {
                    return false;
                }
                self.text.clear();
                self.text.push_str(text);
                return true;
            }
            PropertyValue::Number(number) => number,
        };
        if self.apply_extra(property, number) {
            return true;
        }
        let small = to_i32(number);
        let pixels = to_i16(number);
        let color = ColorValue::from_value(number);
        let settings = &mut self.settings;

        match property {
            Property::X => replace(&mut settings.x, Some(pixels)),
            Property::Y => replace(&mut settings.y, Some(pixels)),
            Property::Width => replace(&mut settings.width, Some(Length::from_value(small))),
            Property::Height => replace(&mut settings.height, Some(Length::from_value(small))),
            Property::Grow => replace(&mut settings.grow, pixels.max(0)),
            Property::Padding => replace(&mut settings.padding, pixels.max(0)),
            Property::Margin => replace(&mut settings.margin, pixels.max(0)),
            Property::Gap => replace(&mut settings.gap, pixels.max(0)),
            Property::Background => color.is_some() && replace(&mut settings.background, color),
            Property::Color => color.is_some() && replace(&mut settings.color, color),
            Property::BorderColor => color.is_some() && replace(&mut settings.border_color, color),
            Property::BorderWidth => replace(&mut settings.border_width, Some(pixels.max(0))),
            Property::Radius => replace(&mut settings.radius, Some(pixels.max(0))),
            Property::Font => replace(&mut settings.font, FontName::from_value(small)),
            Property::Align => replace(&mut settings.align, Align::from_value(small)),
            Property::Justify => replace(
                &mut settings.justify,
                Placement::from_value(small).unwrap_or(Placement::Start),
            ),
            Property::AlignItems => replace(&mut settings.align_items, Placement::from_value(small)),
            Property::LongMode => replace(&mut settings.long_mode, LongMode::from_value(small)),
            Property::Visible => replace(&mut settings.visible, number != 0),
            Property::Disabled => replace(&mut settings.disabled, number != 0),
            Property::Opacity => replace(&mut settings.opacity, number.clamp(0, OPAQUE as i64) as u8),
            Property::Value | Property::RowCount => replace(&mut settings.value, small),
            Property::Minimum => replace(&mut settings.minimum, small),
            Property::Maximum => replace(&mut settings.maximum, small),
            Property::StartAngle => replace(&mut settings.start_angle, Some(pixels)),
            Property::EndAngle => replace(&mut settings.end_angle, Some(pixels)),
            Property::Columns => replace(&mut settings.columns, to_u16(number)),
            Property::Cells | Property::Options | Property::Points | Property::Series => {
                replace(&mut settings.array, to_variable(number))
            }
            _ => false,
        }
    }

    /// Stores a property only some kinds have; says whether it was one and changed.
    fn apply_extra(&mut self, property: Property, number: i64) -> bool {
        let color = ColorValue::from_value(number);
        let length = Some(to_u16(number));
        let Some(extra) = self.extra.get_mut() else {
            return false;
        };
        match extra {
            Extra::CellGrid {
                rows,
                cell_size,
                cell_gap,
                cell_shape,
                palette,
            } => match property {
                Property::Rows => replace(rows, to_u16(number)),
                Property::CellSize => replace(cell_size, to_u16(number)),
                Property::CellGap => replace(cell_gap, to_u16(number)),
                Property::CellShape => replace(cell_shape, number.clamp(0, 2) as u8),
                Property::Palette => replace(palette, to_variable(number)),
                _ => false,
            },
            Extra::Scale {
                ticks,
                major_every,
                skip,
                labels,
                needle_color,
                major_color,
            } => match property {
                Property::Ticks => replace(ticks, to_u16(number)),
                Property::MajorEvery => replace(major_every, to_u16(number)),
                Property::Skip => replace(skip, to_variable(number)),
                Property::Labels => replace(labels, to_variable(number)),
                Property::NeedleColor => color.is_some() && replace(needle_color, color),
                Property::MajorColor => color.is_some() && replace(major_color, color),
                _ => false,
            },
            Extra::AnalogClock {
                time,
                show_seconds,
                hand_style,
                hour_color,
                minute_color,
                second_color,
                hour_length,
                minute_length,
                second_length,
            } => match property {
                Property::Time => replace(time, number),
                Property::ShowSeconds => replace(show_seconds, number != 0),
                Property::HandStyle => replace(hand_style, number.clamp(0, 2) as u8),
                Property::HourColor => color.is_some() && replace(hour_color, color),
                Property::MinuteColor => color.is_some() && replace(minute_color, color),
                Property::SecondColor => color.is_some() && replace(second_color, color),
                Property::HourLength => replace(hour_length, length),
                Property::MinuteLength => replace(minute_length, length),
                Property::SecondLength => replace(second_length, length),
                _ => false,
            },
            Extra::Image { reference, recolor } => match property {
                Property::Image | Property::Icon => replace(reference, Some(to_i32(number))),
                Property::Recolor => color.is_some() && replace(recolor, color),
                _ => false,
            },
            Extra::Repeat { .. } | Extra::None => false,
        }
    }

    /// The colour behind the node's own content, if it paints one.
    pub fn own_background(&self, theme: &Theme) -> Option<Color> {
        let default = match self.kind {
            WidgetKind::Button | WidgetKind::Dropdown => Some(ColorValue::Theme(ThemeColor::Surface)),
            _ => None,
        };
        match self.kind {
            WidgetKind::Arc | WidgetKind::Bar | WidgetKind::Switch | WidgetKind::Slider | WidgetKind::Led => {
                None
            }
            _ => self
                .settings
                .background
                .or(default)
                .map(|color| to_color(color.resolve(theme))),
        }
    }

    /// The state arrays the widget shows, which redraw it when a handler changes them.
    pub fn arrays_read(&self) -> u64 {
        let mut variables = 0u64;
        let mut add = |variable: Option<u8>| {
            if let Some(variable) = variable.filter(|&variable| variable < 64) {
                variables |= 1 << variable;
            }
        };
        add(self.settings.array);
        match *self.extra {
            Extra::CellGrid { palette, .. } => add(palette),
            Extra::Scale { skip, labels, .. } => {
                add(skip);
                add(labels);
            }
            _ => {}
        }
        variables
    }
}

/// A rectangle shrunk by `amount` on every side.
pub fn inset(rect: Rect, amount: i32) -> Rect {
    Rect {
        x: rect.x + amount,
        y: rect.y + amount,
        width: (rect.width - 2 * amount).max(0),
        height: (rect.height - 2 * amount).max(0),
    }
}

/// Whether a widget kind is drawn as itself here, rather than as a plain box.
pub fn is_mapped(kind: WidgetKind) -> bool {
    use WidgetKind::*;
    matches!(
        kind,
        Column
            | Row
            | Stack
            | Grid
            | Scroll
            | List
            | Label
            | Button
            | Arc
            | Bar
            | ClickArea
            | Led
            | Switch
            | Slider
            | CellGrid
            | AppList
            | Repeat
            | Line
            | Scale
            | AnalogClock
            | Chart
            | Image
            | Icon
            | ButtonMatrix
            | Roller
            | Dropdown
            | Checkbox
            | Radio
    )
}

#[cfg(test)]
#[path = "nodes_tests.rs"]
mod nodes_tests;

// ============================================================================
// Repeated rows
// ============================================================================

/// A Repeat's rows when they have one fixed height: the height, how many rows show, and the first built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowSlots {
    pub extent: i32,
    pub count: i32,
    pub first_built: i32,
    pub built: i32,
}

/// The slots of a Repeat whose rows have one fixed height; none for any other node.
pub fn row_slots(node: &Node) -> Option<RowSlots> {
    match *node.extra {
        Extra::Repeat {
            slot_extent,
            slot_count,
            first_built_slot,
            built_slots,
        } if slot_extent > 0 => Some(RowSlots {
            extent: slot_extent.into(),
            count: slot_count.into(),
            first_built: first_built_slot.into(),
            built: built_slots.into(),
        }),
        _ => None,
    }
}

/// The slots of `count` rows that can be seen through `clip`, for a Repeat at `top`: first and one past the last.
pub fn slots_in_view(top: i32, clip: Rect, extent: i32, gap: i32, count: i32) -> (i32, i32) {
    let pitch = (extent + gap).max(1);
    let first = ((clip.y - top) - extent + pitch)
        .div_euclid(pitch)
        .clamp(0, count);
    let last = (clip.y + clip.height - top + pitch - 1)
        .div_euclid(pitch)
        .clamp(first, count);
    (first, last)
}
