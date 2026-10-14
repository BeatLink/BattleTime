//! The pickers and Confirm: widgets that would otherwise be several rollers, buttons and labels,
//! each worked out, drawn and tapped as one node.
//!
//! Each shows its `value` and, when tapped, works out the value the tap makes, which the host
//! sends to its `onChange`.

use alloc::{format, string::String, vec::Vec};

use neotime_runtime::{ThemeColor, Value, WidgetKind};
use neotime_ui::{geometry::Rect, renderer::Renderer};

use super::{
    draw_text_block, fill_rounded_rect, line_height, option_row_height, resolve, roller_rows, text_width,
    DrawContext, BUTTON_RADIUS, MATRIX_RADIUS, OPTION_ROW_PADDING,
};
use crate::nodes::{inset, Align, Extra, Node};

// ============================================================================
// Sizes
// ============================================================================

/// The width of the AM and PM button beside TimeRollers.
const TOGGLE_WIDTH: i32 = 56;

/// The height a button takes when it has room, as a Button's smallest.
const BUTTON_HEIGHT: i32 = 44;

/// The letters on a WeekdayPicker's buttons, Monday first.
const DAY_LETTERS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

/// The space around a Confirm's question and buttons.
const CONFIRM_PADDING: i32 = 16;

// ============================================================================
// Rollers
// ============================================================================

/// One rolling column of numbers, from `first` up `count` numbers, with the chosen one's place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NumberColumn {
    first: i32,
    count: i32,
    chosen: i32,
    digits: usize,
}

/// The columns TimeRollers or DurationRollers show for their value, and the AM or PM button's text.
fn roller_columns(node: &Node) -> (Vec<NumberColumn>, Option<&'static str>) {
    let Extra::Picker {
        twelve_hour,
        show_hours,
        digits,
        ..
    } = *node.extra
    else {
        return (Vec::new(), None);
    };
    let column = |first, count: i32, chosen: i32| NumberColumn {
        first,
        count,
        chosen: chosen.clamp(0, (count - 1).max(0)),
        digits: 2,
    };
    if node.kind == WidgetKind::NumberRoller {
        let (minimum, maximum) = (
            node.settings.minimum,
            node.settings.maximum.max(node.settings.minimum),
        );
        let count = maximum - minimum + 1;
        let only = NumberColumn {
            digits: digits as usize,
            ..column(minimum, count, node.settings.value - minimum)
        };
        return (alloc::vec![only], None);
    }
    let value = node.settings.value.max(0);
    match node.kind {
        WidgetKind::TimeRollers => {
            let minute_of_day = value.min(24 * 60 - 1);
            let (hour, minute) = (minute_of_day / 60, minute_of_day % 60);
            match twelve_hour {
                true => (
                    alloc::vec![column(1, 12, (hour + 11) % 12), column(0, 60, minute)],
                    Some(if hour < 12 { "AM" } else { "PM" }),
                ),
                false => (alloc::vec![column(0, 24, hour), column(0, 60, minute)], None),
            }
        }
        _ => {
            let (hours, minutes, seconds) = (value / 3600, value / 60, value % 60);
            match show_hours {
                true => (
                    alloc::vec![
                        column(0, 24, hours),
                        column(0, 60, minutes % 60),
                        column(0, 60, seconds)
                    ],
                    None,
                ),
                false => (
                    alloc::vec![column(0, 100, minutes % 100), column(0, 60, seconds)],
                    None,
                ),
            }
        }
    }
}

/// The value the columns' chosen numbers make, with the toggle flipped when asked.
fn roller_value(node: &Node, columns: &[NumberColumn], flip_toggle: bool) -> Value {
    let chosen = |place: usize| columns.get(place).map_or(0, |column| column.chosen) as Value;
    match (node.kind, columns.len()) {
        (WidgetKind::NumberRoller, _) => node.settings.minimum as Value + chosen(0),
        (WidgetKind::TimeRollers, _) => {
            let is_twelve_hour = matches!(
                *node.extra,
                Extra::Picker {
                    twelve_hour: true,
                    ..
                }
            );
            let hour = match is_twelve_hour {
                true => {
                    let was_afternoon = node.settings.value / 60 >= 12;
                    let afternoon = was_afternoon != flip_toggle;
                    (chosen(0) + 1) % 12 + if afternoon { 12 } else { 0 }
                }
                false => chosen(0),
            };
            hour * 60 + chosen(1)
        }
        (_, 3) => chosen(0) * 3600 + chosen(1) * 60 + chosen(2),
        _ => chosen(0) * 60 + chosen(1),
    }
}

/// Where each column and separator of TimeRollers or DurationRollers sits, and the toggle's place.
struct RollerPlaces {
    columns: Vec<Rect>,
    separators: Vec<Rect>,
    toggle: Option<Rect>,
    rows: i32,
    row_height: i32,
    top: i32,
}

fn column_width(node: &Node) -> i32 {
    let widest = match (node.kind, *node.extra) {
        (WidgetKind::NumberRoller, Extra::Picker { digits, .. }) => {
            let largest = node.settings.maximum.abs().max(node.settings.minimum.abs());
            format!("{:0width$}", largest, width = digits as usize)
        }
        _ => String::from("00"),
    };
    text_width(node.settings.font.metrics(), &widest) + 4 * OPTION_ROW_PADDING
}

fn separator_width(node: &Node) -> i32 {
    text_width(node.settings.font.metrics(), ":")
}

/// How big TimeRollers or DurationRollers want to be.
pub fn rollers_size(node: &Node) -> (i32, i32) {
    let (columns, toggle) = roller_columns(node);
    let count = columns.len() as i32;
    let gap = i32::from(node.settings.gap);
    let mut width = count * column_width(node) + (count - 1).max(0) * (separator_width(node) + 2 * gap);
    if toggle.is_some() {
        width += gap + TOGGLE_WIDTH;
    }
    (width, 3 * option_row_height(node))
}

fn roller_places(node: &Node, columns: usize, has_toggle: bool) -> RollerPlaces {
    let (width, _) = rollers_size(node);
    let gap = i32::from(node.settings.gap);
    let row_height = option_row_height(node);
    let rows = roller_rows(node);
    let top = node.bounds.y + Align::Center.offset(node.bounds.height, rows * row_height);
    let mut x = node.bounds.x + Align::Center.offset(node.bounds.width, width);
    let mut places = RollerPlaces {
        columns: Vec::new(),
        separators: Vec::new(),
        toggle: None,
        rows,
        row_height,
        top,
    };
    for place in 0..columns {
        if place > 0 {
            x += gap;
            places.separators.push(Rect {
                x,
                y: top,
                width: separator_width(node),
                height: rows * row_height,
            });
            x += separator_width(node) + gap;
        }
        places.columns.push(Rect {
            x,
            y: top,
            width: column_width(node),
            height: rows * row_height,
        });
        x += column_width(node);
    }
    if has_toggle {
        let height = BUTTON_HEIGHT.min(node.bounds.height);
        places.toggle = Some(Rect {
            x: x + gap,
            y: node.bounds.y + Align::Center.offset(node.bounds.height, height),
            width: TOGGLE_WIDTH,
            height,
        });
    }
    places
}

pub fn draw_rollers(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let (columns, toggle) = roller_columns(node);
    let places = roller_places(node, columns.len(), toggle.is_some());
    let theme = context.theme;
    let surface = resolve(None, ThemeColor::Surface, theme);
    let text = resolve(node.settings.color, ThemeColor::Text, theme);
    let muted = resolve(None, ThemeColor::Muted, theme);
    let font = node.settings.font;
    for (column, area) in columns.iter().zip(&places.columns) {
        let band = Rect {
            y: places.top + places.rows / 2 * places.row_height,
            height: places.row_height,
            ..*area
        };
        fill_rounded_rect(renderer, band, surface, MATRIX_RADIUS);
        for row in 0..places.rows {
            let shown = column.chosen + row - places.rows / 2;
            if shown < 0 || shown >= column.count {
                continue;
            }
            let color = if shown == column.chosen { text } else { muted };
            let place = Rect {
                y: places.top + row * places.row_height,
                height: places.row_height,
                ..*area
            };
            let number = format!("{:0width$}", column.first + shown, width = column.digits);
            draw_text_block(renderer, font, &number, place, Align::Center, false, color);
        }
    }
    for separator in &places.separators {
        draw_text_block(renderer, font, ":", *separator, Align::Center, false, text);
    }
    if let (Some(area), Some(label)) = (places.toggle, toggle) {
        fill_rounded_rect(renderer, area, surface, BUTTON_RADIUS as u8);
        draw_text_block(renderer, font, label, area, Align::Center, false, text);
    }
}

/// The value a tap on TimeRollers or DurationRollers makes: a number above or below the chosen one
/// in the column tapped, or the other half of the day; none for a tap between them.
fn rollers_tapped(node: &Node, x: i32, y: i32) -> Option<Value> {
    let (mut columns, toggle) = roller_columns(node);
    let places = roller_places(node, columns.len(), toggle.is_some());
    if places.toggle.is_some_and(|area| contains(area, x, y)) {
        return Some(roller_value(node, &columns, true));
    }
    let place = places.columns.iter().position(|area| contains(*area, x, y))?;
    let row = (y - places.top)
        .div_euclid(places.row_height.max(1))
        .clamp(0, places.rows - 1);
    let column = &mut columns[place];
    column.chosen = (column.chosen + row - places.rows / 2).clamp(0, column.count - 1);
    Some(roller_value(node, &columns, false))
}

// ============================================================================
// Weekdays
// ============================================================================

/// How many columns a WeekdayPicker's buttons are in: its `columns`, or all seven in one row.
fn day_columns(node: &Node) -> i32 {
    match node.settings.columns {
        0 => 7,
        columns => (columns as i32).min(7),
    }
}

fn day_buttons(node: &Node) -> impl Iterator<Item = (usize, Rect)> + '_ {
    let columns = day_columns(node);
    let rows = (7 + columns - 1) / columns;
    let gap = i32::from(node.settings.gap);
    let inner = inset(node.bounds, i32::from(node.settings.padding));
    let width = (inner.width - (columns - 1) * gap) / columns;
    let height = (inner.height - (rows - 1) * gap) / rows;
    (0..7).map(move |day| {
        let (row, column) = (day as i32 / columns, day as i32 % columns);
        let place = Rect {
            x: inner.x + column * (width + gap),
            y: inner.y + row * (height + gap),
            width,
            height,
        };
        (day, place)
    })
}

pub fn weekdays_size(node: &Node) -> (i32, i32) {
    let columns = day_columns(node);
    let rows = (7 + columns - 1) / columns;
    let gap = i32::from(node.settings.gap);
    (
        columns * BUTTON_HEIGHT + (columns - 1) * gap,
        rows * BUTTON_HEIGHT + (rows - 1) * gap,
    )
}

pub fn draw_weekdays(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let theme = context.theme;
    let text = resolve(node.settings.color, ThemeColor::Text, theme);
    for (day, place) in day_buttons(node) {
        let is_set = node.settings.value & (1 << day) != 0;
        let background = resolve(
            None,
            if is_set {
                ThemeColor::Accent
            } else {
                ThemeColor::Surface
            },
            theme,
        );
        fill_rounded_rect(renderer, place, background, BUTTON_RADIUS as u8);
        draw_text_block(
            renderer,
            node.settings.font,
            DAY_LETTERS[day],
            place,
            Align::Center,
            false,
            text,
        );
    }
}

fn weekdays_tapped(node: &Node, x: i32, y: i32) -> Option<Value> {
    let (day, _) = day_buttons(node).find(|(_, place)| contains(*place, x, y))?;
    Some((node.settings.value ^ (1 << day)) as Value)
}

// ============================================================================
// Steppers
// ============================================================================

/// A Stepper's minus button, the place for its value, and its plus button.
///
/// The buttons narrow when the widest value would not fit between them.
fn stepper_places(node: &Node) -> (Rect, Rect, Rect) {
    let inner = inset(node.bounds, i32::from(node.settings.padding));
    let gap = i32::from(node.settings.gap);
    let room_beside_value = (inner.width - widest_stepper_text(node) - 2 * gap) / 2;
    let side = inner.height.min(inner.width / 3).min(room_beside_value).max(0);
    let minus = Rect { width: side, ..inner };
    let plus = Rect {
        x: inner.x + inner.width - side,
        width: side,
        ..inner
    };
    let value = Rect {
        x: minus.x + side + gap,
        width: (inner.width - 2 * side - 2 * gap).max(0),
        ..inner
    };
    (minus, value, plus)
}

/// What a Stepper shows between its buttons: its `text` if it has one, otherwise its value.
fn stepper_text(node: &Node) -> String {
    match node.text.is_empty() {
        true => format!("{}", node.settings.value),
        false => node.text.clone(),
    }
}

/// The width of the widest text a Stepper can show: its `text`, or the wider of its limits and value.
fn widest_stepper_text(node: &Node) -> i32 {
    let font = node.settings.font.metrics();
    let settings = &node.settings;
    let limits = [settings.minimum, settings.maximum].map(|limit| text_width(font, &format!("{limit}")));
    let shown = text_width(font, &stepper_text(node));
    match node.text.is_empty() {
        true => shown.max(limits[0]).max(limits[1]),
        false => shown,
    }
}

pub fn stepper_size(node: &Node) -> (i32, i32) {
    let gap = i32::from(node.settings.gap);
    let text = widest_stepper_text(node);
    let height = BUTTON_HEIGHT.max(line_height(node.settings.font.metrics()));
    (2 * height + 2 * gap + text, height)
}

pub fn draw_stepper(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let theme = context.theme;
    let text = resolve(node.settings.color, ThemeColor::Text, theme);
    let surface = resolve(None, ThemeColor::Surface, theme);
    let (minus, value, plus) = stepper_places(node);
    let font = node.settings.font;
    for (place, sign) in [(minus, "-"), (plus, "+")] {
        fill_rounded_rect(renderer, place, surface, BUTTON_RADIUS as u8);
        draw_text_block(renderer, font, sign, place, Align::Center, false, text);
    }
    draw_text_block(
        renderer,
        font,
        &stepper_text(node),
        value,
        Align::Center,
        false,
        text,
    );
}

fn stepper_tapped(node: &Node, x: i32, y: i32) -> Option<Value> {
    let Extra::Picker { step, .. } = *node.extra else {
        return None;
    };
    let (minus, _, plus) = stepper_places(node);
    let change = match (contains(minus, x, y), contains(plus, x, y)) {
        (true, _) => -(step as i32),
        (_, true) => step as i32,
        _ => return None,
    };
    let settings = &node.settings;
    Some((settings.value + change).clamp(settings.minimum, settings.maximum) as Value)
}

// ============================================================================
// Confirm
// ============================================================================

/// A Confirm's cancel text, confirm text and question, from the lines of its text.
fn confirm_texts(node: &Node) -> (&str, &str, &str) {
    let mut lines = node.text.splitn(3, '\n');
    let cancel = lines.next().filter(|text| !text.is_empty()).unwrap_or("No");
    let confirm = lines.next().filter(|text| !text.is_empty()).unwrap_or("Yes");
    (cancel, confirm, lines.next().unwrap_or(""))
}

/// Where a Confirm's question and its two buttons sit.
fn confirm_places(node: &Node) -> (Rect, Rect, Rect) {
    let inner = inset(node.bounds, CONFIRM_PADDING);
    let (_, _, question) = confirm_texts(node);
    let font = node.settings.font;
    let lines = crate::text::WrappedLines::new(question, font, inner.width.max(1))
        .count()
        .max(1) as i32;
    let question_area = Rect {
        height: lines * line_height(font.metrics()),
        ..inner
    };
    let gap = 8;
    let buttons_top = question_area.y + question_area.height + CONFIRM_PADDING;
    let width = (inner.width - gap) / 2;
    let cancel = Rect {
        x: inner.x,
        y: buttons_top,
        width,
        height: BUTTON_HEIGHT,
    };
    let confirm = Rect {
        x: inner.x + width + gap,
        ..cancel
    };
    (question_area, cancel, confirm)
}

pub fn draw_confirm(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let theme = context.theme;
    let text = resolve(node.settings.color, ThemeColor::Text, theme);
    renderer.fill_rect(
        node.bounds,
        resolve(node.settings.background, ThemeColor::Background, theme),
    );
    let (cancel_text, confirm_text, question) = confirm_texts(node);
    let (question_area, cancel, confirm) = confirm_places(node);
    let font = node.settings.font;
    draw_text_block(renderer, font, question, question_area, Align::Center, true, text);
    for (place, label, color) in [
        (cancel, cancel_text, ThemeColor::Surface),
        (confirm, confirm_text, ThemeColor::Danger),
    ] {
        fill_rounded_rect(renderer, place, resolve(None, color, theme), BUTTON_RADIUS as u8);
        draw_text_block(renderer, font, label, place, Align::Center, false, text);
    }
}

fn confirm_tapped(node: &Node, x: i32, y: i32) -> Option<Value> {
    let (_, cancel, confirm) = confirm_places(node);
    match (contains(cancel, x, y), contains(confirm, x, y)) {
        (true, _) => Some(0),
        (_, true) => Some(1),
        _ => None,
    }
}

// ============================================================================
// Taps
// ============================================================================

fn contains(area: Rect, x: i32, y: i32) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}

/// Whether a widget kind is one of these, which take a tap and answer with `onChange`.
pub fn is_picker(kind: WidgetKind) -> bool {
    matches!(
        kind,
        WidgetKind::TimeRollers
            | WidgetKind::DurationRollers
            | WidgetKind::WeekdayPicker
            | WidgetKind::Stepper
            | WidgetKind::Confirm
            | WidgetKind::NumberRoller
    )
}

/// The value a tap on one of these makes, or none when it misses every part that takes taps.
pub fn picker_tapped(node: &Node, x: i32, y: i32) -> Option<Value> {
    match node.kind {
        WidgetKind::TimeRollers | WidgetKind::DurationRollers | WidgetKind::NumberRoller => {
            rollers_tapped(node, x, y)
        }
        WidgetKind::WeekdayPicker => weekdays_tapped(node, x, y),
        WidgetKind::Stepper => stepper_tapped(node, x, y),
        WidgetKind::Confirm => confirm_tapped(node, x, y),
        _ => None,
    }
}
