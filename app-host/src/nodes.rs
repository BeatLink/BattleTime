//! One node per widget of the open screen: the values its properties hold, where layout put it,
//! and the rlvgl widget that draws it.
//!
//! Colours are kept as the app gave them, fixed or a theme reference, and only resolved when the
//! rlvgl widget is built, so a new theme needs no app code to run.

use alloc::string::String;

use neotime_runtime::{widgets::Property, ColorValue, Theme, ThemeColor, WidgetKind};
use rlvgl_core::{
    renderer::Renderer,
    style::Style,
    widget::{Color, Rect, Widget},
};
use rlvgl_widgets::{
    arc::Arc, bar::Bar, button::Button, click_area::ClickArea, container::Container, label::Label, led::Led,
    slider::Slider, switch::Switch,
};

use crate::fonts::{line_height, text_width, FontName};

// ============================================================================
// Property values
// ============================================================================

/// A length: whole pixels, or a share of the parent's inner size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Length {
    Pixels(i32),
    Percent(i32),
}

impl Length {
    /// A negative value is a percentage, so `-100` means `"100%"`.
    pub fn from_value(value: i32) -> Length {
        match value {
            value if value < 0 => Length::Percent(value.saturating_neg()),
            value => Length::Pixels(value),
        }
    }

    pub fn of(self, whole: i32) -> i32 {
        match self {
            Length::Pixels(pixels) => pixels,
            Length::Percent(percent) => whole.saturating_mul(percent) / 100,
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

/// A property's value once read: a number, or text for `text`.
pub enum PropertyValue<'text> {
    Number(i64),
    Text(&'text str),
}

/// What a widget's properties hold, apart from its text, which its rlvgl widget keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub grow: i32,
    pub padding: i32,
    pub margin: i32,
    pub background: Option<ColorValue>,
    pub color: Option<ColorValue>,
    pub border_color: Option<ColorValue>,
    pub border_width: Option<i32>,
    pub radius: Option<i32>,
    pub font: FontName,
    pub align: Option<Align>,
    pub visible: bool,
    pub value: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub start_angle: Option<i32>,
    pub end_angle: Option<i32>,
}

const DEFAULT_MAXIMUM: i32 = 100;

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
            background: None,
            color: None,
            border_color: None,
            border_width: None,
            radius: None,
            font: FontName::Normal,
            align: None,
            visible: true,
            value: 0,
            minimum: 0,
            maximum: DEFAULT_MAXIMUM,
            start_angle: None,
            end_angle: None,
        }
    }
}

/// Clamps a 64-bit app value into the 32 bits a widget holds.
fn to_i32(value: i64) -> i32 {
    value.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// Stores `new` in `place` and says whether that changed it.
fn replace<Value: PartialEq>(place: &mut Value, new: Value) -> bool {
    let changed = *place != new;
    *place = new;
    changed
}

// ============================================================================
// Default looks
// ============================================================================

const BUTTON_RADIUS: i32 = 12;
const BAR_RADIUS: i32 = 4;
const ARC_STROKE_WIDTH: i32 = 10;

/// App angles count clockwise from twelve o'clock and rlvgl's from three, so arcs turn back 90 degrees.
const TWELVE_TO_RIGHT_IN_DEGREES: i32 = -90;

/// An arc's default gap at the bottom, in app degrees: from about eight o'clock round to four.
const ARC_DEFAULT_START_ANGLE: i32 = 225;
const ARC_DEFAULT_END_ANGLE: i32 = 135;
const BORDER_WIDTH: i32 = 0;
const OPAQUE: u8 = 255;
const TRANSPARENT: Color = Color(0, 0, 0, 0);

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

fn resolve(color: Option<ColorValue>, fallback: ThemeColor, theme: &Theme) -> Color {
    to_color(color.unwrap_or(ColorValue::Theme(fallback)).resolve(theme))
}

fn resolve_or_clear(color: Option<ColorValue>, theme: &Theme) -> Color {
    color.map_or(TRANSPARENT, |color| to_color(color.resolve(theme)))
}

fn to_u8(value: i32) -> u8 {
    value.clamp(0, u8::MAX as i32) as u8
}

// ============================================================================
// Hosted widgets
// ============================================================================

/// The rlvgl widgets that draw one node.
pub enum HostedWidget {
    /// Column, Row, Stack and any widget not mapped yet: a box with an optional background.
    Container(Container),
    Label {
        frame: Container,
        label: Label,
    },
    Button {
        body: Button,
        caption: Label,
    },
    Arc(Arc),
    Bar(Bar),
    ClickArea(ClickArea),
    Led(Led),
    Switch(Switch),
    Slider(Slider),
}

impl HostedWidget {
    pub fn text(&self) -> &str {
        match self {
            HostedWidget::Label { label, .. } => label.text(),
            HostedWidget::Button { caption, .. } => caption.text(),
            _ => "",
        }
    }

    fn set_text(&mut self, text: &str) {
        match self {
            HostedWidget::Label { label, .. } => label.set_text(text),
            HostedWidget::Button { caption, .. } => caption.set_text(text),
            _ => {}
        }
    }

    pub fn draw(&self, renderer: &mut dyn Renderer) {
        match self {
            HostedWidget::Container(container) => container.draw(renderer),
            HostedWidget::Label { frame, label } => {
                frame.draw(renderer);
                label.draw(renderer);
            }
            HostedWidget::Button { body, caption } => {
                body.draw(renderer);
                caption.draw(renderer);
            }
            HostedWidget::Arc(arc) => arc.draw(renderer),
            HostedWidget::Bar(bar) => bar.draw(renderer),
            HostedWidget::ClickArea(click_area) => click_area.draw(renderer),
            HostedWidget::Led(led) => led.draw(renderer),
            HostedWidget::Switch(switch) => switch.draw(renderer),
            HostedWidget::Slider(slider) => slider.draw(renderer),
        }
    }
}

/// Whether a widget kind has an rlvgl widget of its own here, rather than a plain box.
pub fn is_mapped(kind: WidgetKind) -> bool {
    use WidgetKind::*;
    matches!(
        kind,
        Column | Row | Stack | Label | Button | Arc | Bar | ClickArea | Led | Switch | Slider
    )
}

/// A blank widget for a node, before its properties are read and layout places it.
pub fn empty_widget(kind: WidgetKind) -> HostedWidget {
    let nowhere = Rect {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };
    match kind {
        WidgetKind::Label => HostedWidget::Label {
            frame: Container::new(nowhere),
            label: Label::new(String::new(), nowhere),
        },
        WidgetKind::Button => HostedWidget::Button {
            body: Button::new(String::new(), nowhere),
            caption: Label::new(String::new(), nowhere),
        },
        _ => HostedWidget::Container(Container::new(nowhere)),
    }
}

// ============================================================================
// Nodes
// ============================================================================

pub struct Node {
    /// The widget's number in the image.
    pub widget_index: u16,
    pub kind: WidgetKind,
    /// The node just past this one and everything inside it.
    pub subtree_end: u8,
    pub parent: Option<u8>,
    pub settings: Settings,
    pub bounds: Rect,
    /// Whether the widget and every widget around it is visible.
    pub shown: bool,
    /// Where the widget was last drawn, so moving or hiding it can clear that area.
    pub drawn_bounds: Option<Rect>,
    pub widget: HostedWidget,
}

impl Node {
    pub fn text(&self) -> &str {
        self.widget.text()
    }

    /// Stores one property's value and says whether anything visible changed.
    pub fn apply(&mut self, property: Property, value: PropertyValue) -> bool {
        let number = match value {
            PropertyValue::Text(text) => {
                if property != Property::Text || self.widget.text() == text {
                    return false;
                }
                self.widget.set_text(text);
                return true;
            }
            PropertyValue::Number(number) => number,
        };
        let small = to_i32(number);
        let color = ColorValue::from_value(number);
        let settings = &mut self.settings;

        match property {
            Property::X => replace(&mut settings.x, Some(small)),
            Property::Y => replace(&mut settings.y, Some(small)),
            Property::Width => replace(&mut settings.width, Some(Length::from_value(small))),
            Property::Height => replace(&mut settings.height, Some(Length::from_value(small))),
            Property::Grow => replace(&mut settings.grow, small.max(0)),
            Property::Padding => replace(&mut settings.padding, small.max(0)),
            Property::Margin => replace(&mut settings.margin, small.max(0)),
            Property::Background => color.is_some() && replace(&mut settings.background, color),
            Property::Color => color.is_some() && replace(&mut settings.color, color),
            Property::BorderColor => color.is_some() && replace(&mut settings.border_color, color),
            Property::BorderWidth => replace(&mut settings.border_width, Some(small.max(0))),
            Property::Radius => replace(&mut settings.radius, Some(small.max(0))),
            Property::Font => replace(&mut settings.font, FontName::from_value(small)),
            Property::Align => replace(&mut settings.align, Align::from_value(small)),
            Property::Visible => replace(&mut settings.visible, number != 0),
            Property::Value => replace(&mut settings.value, small),
            Property::Minimum => replace(&mut settings.minimum, small),
            Property::Maximum => replace(&mut settings.maximum, small),
            Property::StartAngle => replace(&mut settings.start_angle, Some(small)),
            Property::EndAngle => replace(&mut settings.end_angle, Some(small)),
            _ => false,
        }
    }

    /// Builds the node's rlvgl widgets again from its values, its place and the theme.
    pub fn rebuild_widget(&mut self, theme: &Theme) {
        let text: String = self.widget.text().into();
        self.widget = build_widget(self.kind, &self.settings, text, self.bounds, theme);
    }

    /// The colour behind the node's own content, if it paints one.
    pub fn own_background(&self, theme: &Theme) -> Option<Color> {
        let default = match self.kind {
            WidgetKind::Button => Some(ColorValue::Theme(ThemeColor::Surface)),
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
}

fn frame_style(settings: &Settings, background: Color, default_radius: i32, theme: &Theme) -> Style {
    Style {
        bg_color: background,
        border_color: resolve(settings.border_color, ThemeColor::Muted, theme),
        border_width: to_u8(settings.border_width.unwrap_or(BORDER_WIDTH)),
        alpha: OPAQUE,
        radius: to_u8(settings.radius.unwrap_or(default_radius)),
    }
}

/// Where a line of text sits inside a box, by `align` across and centred down.
pub fn text_box(bounds: Rect, settings: &Settings, text: &str, default_align: Align) -> Rect {
    let font = settings.font.metrics();
    let inner = inset(bounds, settings.padding);
    let width = text_width(font, text).min(inner.width.max(0));
    let height = line_height(font);
    let align = settings.align.unwrap_or(default_align);
    Rect {
        x: inner.x + align.offset(inner.width, width),
        y: inner.y + Align::Center.offset(inner.height, height),
        width,
        height,
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

fn caption_label(text: String, place: Rect, settings: &Settings, theme: &Theme) -> Label {
    let mut label = Label::new(text, place);
    label.style.bg_color = TRANSPARENT;
    label.set_font(settings.font.metrics());
    label.set_text_color(resolve(settings.color, ThemeColor::Text, theme));
    label
}

fn build_widget(
    kind: WidgetKind,
    settings: &Settings,
    text: String,
    bounds: Rect,
    theme: &Theme,
) -> HostedWidget {
    let accent = |color: Option<ColorValue>| resolve(color, ThemeColor::Accent, theme);
    let track = |color: Option<ColorValue>| resolve(color, ThemeColor::Surface, theme);

    match kind {
        WidgetKind::Label => {
            let mut frame = Container::new(bounds);
            frame.style = frame_style(settings, resolve_or_clear(settings.background, theme), 0, theme);
            let place = text_box(bounds, settings, &text, Align::Start);
            HostedWidget::Label {
                frame,
                label: caption_label(text, place, settings, theme),
            }
        }
        WidgetKind::Button => {
            let mut body = Button::new(String::new(), bounds);
            *body.style_mut() = frame_style(settings, track(settings.background), BUTTON_RADIUS, theme);
            let place = text_box(bounds, settings, &text, Align::Center);
            HostedWidget::Button {
                body,
                caption: caption_label(text, place, settings, theme),
            }
        }
        WidgetKind::Arc => {
            let mut arc = Arc::new(bounds, settings.minimum, settings.maximum);
            let (start, end) = match (settings.start_angle, settings.end_angle) {
                (Some(start), Some(end)) => (start, end),
                _ => (ARC_DEFAULT_START_ANGLE, ARC_DEFAULT_END_ANGLE),
            };
            arc.set_bg_angles(start, end);
            arc.set_rotation(TWELVE_TO_RIGHT_IN_DEGREES);
            arc.set_value(settings.value);
            arc.style.border_color = track(settings.background);
            arc.style.border_width = to_u8(settings.border_width.unwrap_or(ARC_STROKE_WIDTH));
            arc.indicator_color = accent(settings.color);
            arc.knob_color = TRANSPARENT;
            HostedWidget::Arc(arc)
        }
        WidgetKind::Bar => {
            let mut bar = Bar::new(bounds, settings.minimum, settings.maximum);
            bar.set_value(settings.value);
            bar.style = frame_style(settings, track(settings.background), BAR_RADIUS, theme);
            bar.indicator_color = accent(settings.color);
            HostedWidget::Bar(bar)
        }
        WidgetKind::Led => {
            let mut led = Led::new(bounds);
            led.set_color(accent(settings.color));
            led.set_brightness(to_u8(settings.value));
            HostedWidget::Led(led)
        }
        WidgetKind::Switch => {
            let mut switch = Switch::new(bounds);
            let is_on = settings.value != 0;
            let on_track = if is_on {
                accent(settings.background)
            } else {
                track(settings.background)
            };
            switch.style = frame_style(settings, on_track, bounds.height / 2, theme);
            switch.knob_color = resolve(settings.color, ThemeColor::Text, theme);
            switch.set_on(is_on);
            HostedWidget::Switch(switch)
        }
        WidgetKind::Slider => {
            let mut slider = Slider::new(bounds, settings.minimum, settings.maximum);
            slider.style = frame_style(settings, track(settings.background), bounds.height / 2, theme);
            slider.knob_color = accent(settings.color);
            slider.set_value(settings.value);
            HostedWidget::Slider(slider)
        }
        WidgetKind::ClickArea => HostedWidget::ClickArea(ClickArea::new(bounds)),
        _ => {
            let mut container = Container::new(bounds);
            container.style = frame_style(settings, resolve_or_clear(settings.background, theme), 0, theme);
            HostedWidget::Container(container)
        }
    }
}
