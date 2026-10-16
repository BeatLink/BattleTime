//! Draws one node from its values: value widgets through `widgets`, and the rest, such as
//! the ones the host shapes itself, such as cell grids, dials and lists of options, straight from the app's state.
//!
//! Array-backed widgets read their arrays at draw time rather than copying them, so a 400-cell
//! board costs no more UI heap than a label.

use alloc::{string::String, vec::Vec};

use battletime_runtime::{
    library_icons::LIBRARY_ICON_SIDE,
    resources::{library_image_named_by, ImageFormat, ImageReference, ImageResource},
    AppImage, AppState, ColorValue, Theme, ThemeColor, Value,
};
use battletime_ui::{
    geometry::{Color, PointF, Rect},
    renderer::Renderer,
    shapes::{draw_widget_bg, fill_rounded_rect, Style},
};

use crate::bitmap_file::ShownBitmap;
use crate::fonts::{self, line_height, text_width, FontName};
use crate::layout::{has_page_bar, visible_length, TICK_BOX_GAP, TICK_BOX_SIZE};
use crate::nodes::{
    inset, resolve, resolve_or_clear, to_color, Align, Extra, LongMode, Node, Settings, OPAQUE, TRANSPARENT,
};
use crate::text::{draw_line, WrappedLines};
use crate::widgets::{draw_arc, draw_bar, draw_led, draw_slider, draw_switch, ArcLook};

// ============================================================================
// What drawing needs
// ============================================================================

/// What a node reads while it is drawn, besides its own values.
#[derive(Clone, Copy)]
pub struct DrawContext<'context, 'image> {
    pub image: &'context AppImage<'image>,
    pub library: Option<&'context AppImage<'image>>,
    /// Each installed app's icon as `AppImage::resource_bytes` gives it, or empty for none.
    pub installed_app_icons: &'image [&'image [u8]],
    pub state: &'context AppState,
    pub theme: &'context Theme,
    /// Milliseconds the local time is ahead of UTC.
    pub utc_offset: Value,
    /// The time of the last tick, for scrolling labels.
    pub now: Value,
    /// The part of the screen being painted, so a widget with much to draw can skip the rest.
    pub clip: Rect,
    /// Each BitmapFile's picture, empty when its file is missing or not a picture.
    pub bitmap_files: &'context [ShownBitmap],
    /// The shared library's icons the screen shows, read from the icons file before painting.
    pub library_icons: &'context [HeldIcon],
}

/// One of the shared library's icons, read from the icons file for the screen that shows it.
pub struct HeldIcon {
    pub number: u16,
    pub bytes: Vec<u8>,
}

impl<'context, 'image> DrawContext<'context, 'image> {
    /// An array's length and its elements, or nothing if the variable is not an array.
    fn array(&self, variable: Option<u8>) -> Option<(u8, usize)> {
        let variable = variable?;
        let length = self.state.placement(variable)?.length as usize;
        (length > 0).then_some((variable, length))
    }

    fn element(&self, variable: u8, index: usize) -> Value {
        self.state.element(variable, index as Value).unwrap_or(0)
    }

    /// An element that holds fixed text, which is a string of the app's image.
    fn text_element(&self, variable: u8, index: usize) -> &str {
        u16::try_from(self.element(variable, index))
            .ok()
            .and_then(|string| self.image.string(string))
            .unwrap_or("")
    }

    /// The image an `image` or `icon` value names, from the app or the shared library's icons.
    pub fn image_resource(&self, value: i32) -> Option<ImageResource<'context>> {
        let bytes = match ImageReference::from_value(value)? {
            ImageReference::App(index) => self.image.resource_bytes(index)?,
            ImageReference::SharedLibrary(index) => return self.library_icon(index),
            ImageReference::InstalledApp(index) => self.installed_app_icons.get(index as usize)?,
        };
        match library_image_named_by(bytes) {
            Some(library_image) => self.library_icon(library_image),
            None => ImageResource::parse(bytes),
        }
    }

    /// The library icon an image value names, itself or through an app's image or icon that names one.
    pub fn library_icon_named(&self, value: i32) -> Option<u16> {
        let bytes = match ImageReference::from_value(value)? {
            ImageReference::App(index) => self.image.resource_bytes(index)?,
            ImageReference::SharedLibrary(index) => return Some(index),
            ImageReference::InstalledApp(index) => self.installed_app_icons.get(index as usize)?,
        };
        library_image_named_by(bytes)
    }

    /// A library icon held for the screen, or the library's own when it still carries its icons.
    fn library_icon(&self, number: u16) -> Option<ImageResource<'context>> {
        match self.library_icons.iter().find(|icon| icon.number == number) {
            Some(icon) => ImageResource::parse(&icon.bytes),
            None => self.library?.resource(number),
        }
    }

    /// The size of the image a value names, which for a library icon not yet read is the side all of them share.
    pub fn image_size(&self, value: i32) -> Option<(i32, i32)> {
        if let Some(image) = self.image_resource(value) {
            return Some((image.width as i32, image.height as i32));
        }
        let side = LIBRARY_ICON_SIDE as i32;
        self.library_icon_named(value).map(|_| (side, side))
    }
}

/// Hands `visit` each shared library icon a node draws, so they can be read before it is painted.
pub fn for_each_library_icon_drawn(node: &Node, context: &DrawContext, visit: &mut dyn FnMut(u16)) {
    let mut named = |value: i32| {
        if let Some(number) = context.library_icon_named(value) {
            visit(number);
        }
    };
    match *node.extra {
        Extra::Image {
            reference: Some(reference),
            ..
        }
        | Extra::Badge {
            reference: Some(reference),
            ..
        } => named(reference),
        Extra::Battery { .. } => {
            for piece in status_pieces(node).pieces.iter().flatten() {
                match *piece {
                    StatusPiece::Icon { library_icon, .. } => {
                        named(ImageReference::SharedLibrary(library_icon).to_value())
                    }
                    StatusPiece::Bolt => named(
                        ImageReference::SharedLibrary(battletime_runtime::resources::library_icon::CHARGING)
                            .to_value(),
                    ),
                    StatusPiece::Battery | StatusPiece::Percent => {}
                }
            }
        }
        Extra::AppList { .. } => {
            for place in app_list_rows(node).iter().filter_map(|item| item.place) {
                named(ImageReference::InstalledApp(place as u16).to_value());
            }
        }
        _ => {}
    }
}

// ============================================================================
// Default looks
// ============================================================================

const BUTTON_RADIUS: i32 = 12;
const BAR_RADIUS: i32 = 4;
const ARC_STROKE_WIDTH: i32 = 10;
const BORDER_WIDTH: i32 = 0;
const TICK_BOX_RADIUS: u8 = 4;
const TICK_BOX_BORDER: u8 = 2;
const MATRIX_GAP: i32 = 4;
const MATRIX_RADIUS: u8 = 8;
const OPTION_ROW_PADDING: i32 = 8;
const DROPDOWN_ARROW_SIZE: f32 = 6.0;
pub const PLACEHOLDER_SIZE: i32 = 24;
const LINE_WIDTH: f32 = 2.0;
const CHART_LINE_WIDTH: f32 = 2.0;
const SCALE_MAJOR_TICK_LENGTH: f32 = 12.0;
const SCALE_MINOR_TICK_LENGTH: f32 = 6.0;
const SCALE_MAJOR_TICK_WIDTH: f32 = 3.0;
const SCALE_MINOR_TICK_WIDTH: f32 = 1.5;
const SCALE_LABEL_SPACE: f32 = 6.0;
const NEEDLE_WIDTH: f32 = 3.0;
const HOUR_HAND_WIDTH: f32 = 6.0;
const MINUTE_HAND_WIDTH: f32 = 4.0;
const SECOND_HAND_WIDTH: f32 = 2.0;
const CLOCK_CAP_RADIUS: f32 = 5.0;
const HAND_SHARE_PERCENT: [i32; 3] = [50, 75, 85];

/// App angles count clockwise from twelve o'clock and arc angles from three, so arcs turn back 90 degrees.
const TWELVE_TO_RIGHT_IN_DEGREES: i32 = -90;

/// An arc's default gap at the bottom, in app degrees: from about eight o'clock round to four.
const ARC_DEFAULT_START_ANGLE: i32 = 225;
const ARC_DEFAULT_END_ANGLE: i32 = 135;

/// How fast a label with `long_mode` scroll moves, and how long it rests at each end.
pub const LABEL_SCROLL_PIXELS_PER_SECOND: i64 = 30;
pub const LABEL_SCROLL_PAUSE_IN_MILLISECONDS: i64 = 1000;
pub const LABEL_SCROLL_STEP_IN_MILLISECONDS: i64 = 100;

fn to_u8(value: i32) -> u8 {
    value.clamp(0, u8::MAX as i32) as u8
}

fn frame_style(settings: &Settings, background: Color, default_radius: i32, theme: &Theme) -> Style {
    Style {
        bg_color: background,
        border_color: resolve(settings.border_color, ThemeColor::Muted, theme),
        border_width: to_u8(settings.border_width.get().map(i32::from).unwrap_or(BORDER_WIDTH)),
        alpha: OPAQUE,
        radius: to_u8(settings.radius.get().map(i32::from).unwrap_or(default_radius)),
    }
}

fn with_alpha(color: Color, alpha: u8) -> Color {
    Color(color.0, color.1, color.2, alpha)
}

// ============================================================================
// Angles
// ============================================================================

const DEGREES_TO_RADIANS: f32 = core::f32::consts::PI / 180.0;

/// The sine of an angle in degrees, from a polynomial, since the watch has no maths library.
fn sine(degrees: f32) -> f32 {
    let mut turned = degrees % 360.0;
    if turned < 0.0 {
        turned += 360.0;
    }
    let (folded, sign) = match turned {
        angle if angle <= 90.0 => (angle, 1.0),
        angle if angle <= 180.0 => (180.0 - angle, 1.0),
        angle if angle <= 270.0 => (angle - 180.0, -1.0),
        angle => (360.0 - angle, -1.0),
    };
    let x = folded * DEGREES_TO_RADIANS;
    let squared = x * x;
    let value = x * (1.0 - squared / 6.0 * (1.0 - squared / 20.0 * (1.0 - squared / 42.0)));
    sign * value
}

fn cosine(degrees: f32) -> f32 {
    sine(degrees + 90.0)
}

/// The point `radius` from `centre` at an app angle: clockwise from twelve o'clock.
pub fn point_on_circle(centre: PointF, radius: f32, degrees: f32) -> PointF {
    PointF::new(
        centre.x + radius * sine(degrees),
        centre.y - radius * cosine(degrees),
    )
}

fn centre_of(bounds: Rect) -> PointF {
    PointF::new(
        bounds.x as f32 + bounds.width as f32 / 2.0,
        bounds.y as f32 + bounds.height as f32 / 2.0,
    )
}

fn round_line(renderer: &mut dyn Renderer, from: PointF, to: PointF, width: f32, color: Color) {
    renderer.stroke_line_aa(from, to, width, color);
    if width > 2.0 {
        renderer.fill_disc_aa(from, width / 2.0, color);
        renderer.fill_disc_aa(to, width / 2.0, color);
    }
}

// ============================================================================
// Drawing a node
// ============================================================================

/// Draws one node into `renderer`, which the caller has already clipped.
pub fn draw_node(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let settings = &node.settings;
    let theme = context.theme;
    let bounds = node.bounds();
    let accent = |color: Option<ColorValue>| resolve(color, ThemeColor::Accent, theme);
    let track = |color: Option<ColorValue>| resolve(color, ThemeColor::Surface, theme);
    let plain_frame = |renderer: &mut dyn Renderer| {
        let style = frame_style(settings, resolve_or_clear(settings.background, theme), 0, theme);
        draw_widget_bg(renderer, bounds, &style);
    };

    use battletime_runtime::WidgetKind::*;
    match node.kind {
        Label => {
            plain_frame(renderer);
            draw_label_text(node, context, renderer);
        }
        Button => {
            let style = frame_style(settings, track(settings.background), BUTTON_RADIUS, theme);
            draw_widget_bg(renderer, bounds, &style);
            draw_button_content(node, context, renderer);
        }
        Arc => {
            let (start_angle, end_angle) = match (
                settings.start_angle.get().map(i32::from),
                settings.end_angle.get().map(i32::from),
            ) {
                (Some(start), Some(end)) => (start, end),
                _ => (ARC_DEFAULT_START_ANGLE, ARC_DEFAULT_END_ANGLE),
            };
            let look = ArcLook {
                minimum: settings.minimum,
                maximum: settings.maximum,
                value: settings.value,
                start_angle,
                end_angle,
                rotation: TWELVE_TO_RIGHT_IN_DEGREES,
                stroke_width: to_u8(
                    settings
                        .border_width
                        .get()
                        .map(i32::from)
                        .unwrap_or(ARC_STROKE_WIDTH),
                ),
                track_color: track(settings.background),
                indicator_color: accent(settings.color),
            };
            draw_arc(renderer, bounds, &look);
        }
        Bar => {
            let style = frame_style(settings, track(settings.background), BAR_RADIUS, theme);
            let range = (settings.minimum, settings.maximum, settings.value);
            draw_bar(renderer, bounds, &style, range, accent(settings.color));
        }
        Led => {
            draw_led(renderer, bounds, accent(settings.color), to_u8(settings.value));
        }
        Switch => {
            let is_on = settings.value != 0;
            // An off track is muted rather than surface, so it still shows on a surface-coloured row.
            let track_color = match is_on {
                true => accent(settings.background),
                false => resolve(settings.background, ThemeColor::Muted, theme),
            };
            let style = frame_style(settings, track_color, bounds.height / 2, theme);
            let knob_color = resolve(settings.color, ThemeColor::Text, theme);
            draw_switch(renderer, bounds, &style, knob_color, is_on);
        }
        Slider => {
            let style = frame_style(settings, track(settings.background), bounds.height / 2, theme);
            let range = (settings.minimum, settings.maximum, settings.value);
            draw_slider(renderer, bounds, &style, range, accent(settings.color));
        }
        ClickArea => {}
        Checkbox | Radio => draw_tick_box(node, context, renderer),
        CellGrid => {
            plain_frame(renderer);
            draw_cell_grid(node, context, renderer);
        }
        AppList => {
            plain_frame(renderer);
            draw_app_list(node, context, renderer);
        }
        TextFile => {
            plain_frame(renderer);
            draw_text_file(node, context, renderer);
        }
        BitmapFile => {
            plain_frame(renderer);
            draw_bitmap_file(node, context, renderer);
        }
        TextLines => {
            plain_frame(renderer);
            let color = resolve(node.settings.color, ThemeColor::Text, context.theme);
            for (line, place) in text_lines_shown(node, context) {
                draw_text_block(
                    renderer,
                    node.settings.font,
                    &line,
                    place,
                    Align::Start,
                    false,
                    color,
                );
            }
        }
        Battery | StatusIcons => {
            plain_frame(renderer);
            draw_status(node, context, renderer);
        }
        Badge => {
            plain_frame(renderer);
            draw_badge(node, context, renderer);
        }
        TimeRollers | DurationRollers | NumberRoller => {
            plain_frame(renderer);
            pickers::draw_rollers(node, context, renderer);
        }
        WeekdayPicker => {
            plain_frame(renderer);
            pickers::draw_weekdays(node, context, renderer);
        }
        Stepper => {
            plain_frame(renderer);
            pickers::draw_stepper(node, context, renderer);
        }
        Confirm => pickers::draw_confirm(node, context, renderer),
        Line => draw_polyline(node, context, renderer),
        Chart => {
            plain_frame(renderer);
            draw_chart(node, context, renderer);
        }
        Scale => {
            plain_frame(renderer);
            draw_scale(node, context, renderer);
        }
        AnalogClock => {
            plain_frame(renderer);
            draw_analog_clock(node, context, renderer);
        }
        Image | Icon => {
            plain_frame(renderer);
            draw_image_widget(
                node,
                context,
                inset(bounds, i32::from(settings.padding)),
                renderer,
            );
        }
        ButtonMatrix => {
            plain_frame(renderer);
            draw_button_matrix(node, context, renderer);
        }
        Roller => {
            plain_frame(renderer);
            draw_roller(node, context, renderer);
        }
        Dropdown => draw_dropdown(node, context, renderer),
        Pages => {
            plain_frame(renderer);
            if has_page_bar(node) {
                draw_page_bar(node, context, renderer);
            }
        }
        _ => plain_frame(renderer),
    }
}

// ============================================================================
// Text
// ============================================================================

/// How far a scrolling label has moved at `now`: it slides to its end, rests, and slides back.
pub fn label_scroll_offset(overflow: i32, now: Value) -> i32 {
    if overflow <= 0 {
        return 0;
    }
    let travel_in_milliseconds = overflow as i64 * 1000 / LABEL_SCROLL_PIXELS_PER_SECOND;
    let leg = travel_in_milliseconds + LABEL_SCROLL_PAUSE_IN_MILLISECONDS;
    let position = now.rem_euclid(2 * leg);
    let moved = |elapsed: i64| {
        (elapsed.clamp(0, travel_in_milliseconds) * LABEL_SCROLL_PIXELS_PER_SECOND / 1000) as i32
    };
    match position < leg {
        true => moved(position - LABEL_SCROLL_PAUSE_IN_MILLISECONDS),
        false => overflow - moved(position - leg - LABEL_SCROLL_PAUSE_IN_MILLISECONDS),
    }
}

/// Whether a label scrolls: `long_mode` scroll with text wider than its box.
pub fn label_scrolls(node: &Node) -> bool {
    node.kind == battletime_runtime::WidgetKind::Label
        && node.settings.long_mode == LongMode::Scroll
        && text_width(node.settings.font.metrics(), &node.text)
            > inset(node.bounds(), i32::from(node.settings.padding)).width
}

/// Draws lines of text inside `area`, each placed across by `align` and the block centred down.
fn draw_text_block(
    renderer: &mut dyn Renderer,
    font: FontName,
    text: &str,
    area: Rect,
    align: Align,
    wraps: bool,
    color: Color,
) {
    let metrics = font.metrics();
    let height = line_height(metrics);
    let ascent = metrics.line_metrics().ascent as i32;
    let wrap_width = if wraps { area.width.max(1) } else { i32::MAX };
    let line_count = WrappedLines::new(text, font, wrap_width).count().max(1) as i32;
    let mut top = area.y + Align::Center.offset(area.height, line_count * height);
    for line in WrappedLines::new(text, font, wrap_width) {
        let width = text_width(metrics, line);
        let left = area.x + align.offset(area.width, width).max(0);
        draw_line(renderer, font, line, left, top + ascent, color);
        top += height;
    }
}

fn draw_label_text(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let settings = &node.settings;
    let area = inset(node.bounds(), i32::from(settings.padding));
    let color = resolve(settings.color, ThemeColor::Text, context.theme);
    let align = settings.align.unwrap_or(Align::Start);
    let font = settings.font;
    match settings.long_mode {
        LongMode::Wrap => draw_text_block(renderer, font, &node.text, area, align, true, color),
        LongMode::Clip => draw_text_block(renderer, font, &node.text, area, align, false, color),
        LongMode::Scroll => {
            let width = text_width(font.metrics(), &node.text);
            let overflow = width - area.width;
            if overflow <= 0 {
                draw_text_block(renderer, font, &node.text, area, align, false, color);
                return;
            }
            let shifted = Rect {
                x: area.x - label_scroll_offset(overflow, context.now),
                width,
                ..area
            };
            draw_text_block(renderer, font, &node.text, shifted, Align::Start, false, color);
        }
    }
}

/// A button's icon, if it has one, then its text, together centred.
fn draw_button_content(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let settings = &node.settings;
    let area = inset(node.bounds(), i32::from(settings.padding));
    let color = resolve(settings.color, ThemeColor::Text, context.theme);
    let icon = match *node.extra {
        Extra::Image {
            reference: Some(reference),
            ..
        } => Some(reference),
        _ => None,
    };
    let Some(reference) = icon else {
        draw_text_block(
            renderer,
            settings.font,
            &node.text,
            area,
            settings.align.unwrap_or(Align::Center),
            true,
            color,
        );
        return;
    };
    let (icon_width, icon_height) = context
        .image_size(reference)
        .unwrap_or((PLACEHOLDER_SIZE, PLACEHOLDER_SIZE));
    let text_space = text_width(settings.font.metrics(), &node.text);
    let space = if text_space > 0 { TICK_BOX_GAP } else { 0 };
    let total = icon_width + space + text_space;
    let left = area.x + Align::Center.offset(area.width, total);
    let icon_area = Rect {
        x: left,
        y: area.y + Align::Center.offset(area.height, icon_height),
        width: icon_width,
        height: icon_height,
    };
    draw_image(context, reference, icon_area, color, renderer);
    let text_area = Rect {
        x: left + icon_width + space,
        width: text_space,
        ..area
    };
    draw_text_block(
        renderer,
        settings.font,
        &node.text,
        text_area,
        Align::Start,
        false,
        color,
    );
}

// ============================================================================
// Checkboxes and radios
// ============================================================================

fn draw_tick_box(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let settings = &node.settings;
    let theme = context.theme;
    let area = inset(node.bounds(), i32::from(settings.padding));
    let is_on = settings.value != 0;
    let accent = resolve(settings.background, ThemeColor::Accent, theme);
    let border = resolve(settings.border_color, ThemeColor::Muted, theme);
    let text_color = resolve(settings.color, ThemeColor::Text, theme);
    let square = Rect {
        x: area.x,
        y: area.y + Align::Center.offset(area.height, TICK_BOX_SIZE),
        width: TICK_BOX_SIZE,
        height: TICK_BOX_SIZE,
    };
    let centre = centre_of(square);

    if node.kind == battletime_runtime::WidgetKind::Radio {
        let radius = TICK_BOX_SIZE as f32 / 2.0;
        renderer.fill_disc_aa(centre, radius, if is_on { accent } else { border });
        renderer.fill_disc_aa(
            centre,
            radius - TICK_BOX_BORDER as f32,
            resolve(None, ThemeColor::Background, theme),
        );
        if is_on {
            renderer.fill_disc_aa(centre, radius / 2.0, accent);
        }
    } else if is_on {
        fill_rounded_rect(renderer, square, accent, TICK_BOX_RADIUS);
        let tick = resolve(None, ThemeColor::Background, theme);
        let point = |x: f32, y: f32| PointF::new(square.x as f32 + x, square.y as f32 + y);
        round_line(renderer, point(6.0, 12.5), point(10.5, 17.0), 3.0, tick);
        round_line(renderer, point(10.5, 17.0), point(18.0, 7.5), 3.0, tick);
    } else {
        let style = Style {
            bg_color: TRANSPARENT,
            border_color: border,
            border_width: TICK_BOX_BORDER,
            alpha: OPAQUE,
            radius: TICK_BOX_RADIUS,
        };
        draw_widget_bg(renderer, square, &style);
    }

    let text_area = Rect {
        x: square.x + TICK_BOX_SIZE + TICK_BOX_GAP,
        width: (area.width - TICK_BOX_SIZE - TICK_BOX_GAP).max(0),
        ..area
    };
    draw_text_block(
        renderer,
        settings.font,
        &node.text,
        text_area,
        Align::Start,
        false,
        text_color,
    );
}

// ============================================================================
// Cell grids
// ============================================================================

#[path = "pickers.rs"]
pub mod pickers;

// ============================================================================
// Battery, status icons and badges
// ============================================================================

const BOLT_SIZE: i32 = 16;
const PIECE_GAP: i32 = 4;
const BATTERY_WIDTH: i32 = 28;
const BATTERY_HEIGHT: i32 = 14;
const BATTERY_BORDER: i32 = 2;
const BATTERY_RADIUS: i32 = 3;
/// How wide the charge inside a full battery is.
const BATTERY_FILL_WIDTH: i32 = 20;

/// One thing a Battery or StatusIcons draws, left to right.
#[derive(Clone, Copy)]
enum StatusPiece {
    Icon { library_icon: u16, side: i32 },
    Bolt,
    Battery,
    Percent,
}

/// What a Battery or StatusIcons draws, with the gaps between and the tallest piece's height.
struct StatusPieces {
    pieces: [Option<StatusPiece>; 6],
    gap: i32,
    height: i32,
    percent_width: i32,
}

impl StatusPieces {
    fn piece_width(&self, piece: StatusPiece) -> i32 {
        match piece {
            StatusPiece::Icon { side, .. } => side,
            StatusPiece::Bolt => BOLT_SIZE,
            StatusPiece::Battery => BATTERY_WIDTH,
            StatusPiece::Percent => self.percent_width,
        }
    }

    /// The gap before a piece: the icons' own gap, and the battery's inner gap between its own parts.
    fn gap_before(&self, piece: StatusPiece, previous: Option<StatusPiece>) -> i32 {
        match (previous, piece) {
            (None, _) => 0,
            (Some(StatusPiece::Icon { .. }), _) => self.gap,
            _ => PIECE_GAP,
        }
    }

    fn width(&self) -> i32 {
        let mut width = 0;
        let mut previous = None;
        for piece in self.pieces.iter().flatten() {
            width += self.gap_before(*piece, previous) + self.piece_width(*piece);
            previous = Some(*piece);
        }
        width
    }
}

fn status_pieces(node: &Node) -> StatusPieces {
    let Extra::Battery {
        charging,
        show_bolt,
        show_percent,
        status_flags,
        icon_size,
        ..
    } = *node.extra
    else {
        return StatusPieces {
            pieces: [None; 6],
            gap: 0,
            height: 0,
            percent_width: 0,
        };
    };
    use battletime_runtime::resources::library_icon;
    let side = icon_size as i32;
    let has_icons = node.kind == battletime_runtime::WidgetKind::StatusIcons && status_flags & 0b111 != 0;
    let mut pieces = [None; 6];
    let mut count = 0;
    let mut add = |piece: StatusPiece| {
        pieces[count] = Some(piece);
        count += 1;
    };
    if node.kind == battletime_runtime::WidgetKind::StatusIcons {
        for (flag, library_icon) in [
            (1, library_icon::NOTIFICATION),
            (2, library_icon::BLUETOOTH),
            (4, library_icon::ALARM),
        ] {
            if status_flags & flag != 0 {
                add(StatusPiece::Icon { library_icon, side });
            }
        }
    }
    if charging && show_bolt {
        add(StatusPiece::Bolt);
    }
    add(StatusPiece::Battery);
    if show_percent {
        add(StatusPiece::Percent);
    }
    let small = FontName::Small.metrics();
    let mut height = BATTERY_HEIGHT;
    if charging && show_bolt {
        height = height.max(BOLT_SIZE);
    }
    if has_icons {
        height = height.max(side);
    }
    if show_percent {
        height = height.max(line_height(small));
    }
    StatusPieces {
        pieces,
        gap: i32::from(node.settings.gap),
        height,
        percent_width: text_width(small, &percent_text(node.settings.value)),
    }
}

fn percent_text(percent: i32) -> alloc::string::String {
    alloc::format!("{percent}%")
}

/// Draws an image scaled into a square and centred in it, as an `Icon` of that size shows it.
fn draw_icon_in(
    context: &DrawContext,
    reference: i32,
    square: Rect,
    color: Color,
    renderer: &mut dyn Renderer,
) {
    let Some(image) = context.image_resource(reference) else {
        return;
    };
    let (width, height) = (image.width as i32, image.height as i32);
    let (shown_width, shown_height) = if square.width * height <= square.height * width {
        (
            square.width.max(1),
            ((height * square.width + width / 2) / width).max(1),
        )
    } else {
        (
            ((width * square.height + height / 2) / height).max(1),
            square.height.max(1),
        )
    };
    let place = Rect {
        x: square.x + Align::Center.offset(square.width, shown_width),
        y: square.y + Align::Center.offset(square.height, shown_height),
        width: shown_width,
        height: shown_height,
    };
    draw_resource(&image, place, color, false, renderer);
}

/// A Battery drawn as the system draws one, or a StatusIcons' icons followed by its battery.
fn draw_status(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let Extra::Battery {
        charging,
        single_color,
        ..
    } = *node.extra
    else {
        return;
    };
    let theme = context.theme;
    let pieces = status_pieces(node);
    let inner = inset(node.bounds(), i32::from(node.settings.padding));
    let percent = node.settings.value.clamp(0, 100);
    let is_status = node.kind == battletime_runtime::WidgetKind::StatusIcons;
    let icon_color = resolve(node.settings.color, ThemeColor::Text, theme);
    let battery_value = if is_status {
        node.settings.border_color
    } else {
        node.settings.color
    };
    let battery_color = resolve(battery_value, ThemeColor::Text, theme);
    let fill_color = match (single_color, charging, percent <= 20) {
        (true, _, _) => battery_color,
        (false, true, _) => resolve(None, ThemeColor::Success, theme),
        (false, false, true) => resolve(None, ThemeColor::Danger, theme),
        (false, false, false) => battery_color,
    };
    let bolt_color = match single_color {
        true => battery_color,
        false => resolve(None, ThemeColor::Warning, theme),
    };

    let align = node.settings.align.unwrap_or(Align::Start);
    let mut x = inner.x + align.offset(inner.width, pieces.width());
    let small = FontName::Small;
    let mut previous = None;
    for piece in pieces.pieces.iter().flatten().copied() {
        x += pieces.gap_before(piece, previous);
        let width = pieces.piece_width(piece);
        let piece_height = match piece {
            StatusPiece::Icon { side, .. } => side,
            StatusPiece::Bolt => BOLT_SIZE,
            StatusPiece::Battery => BATTERY_HEIGHT,
            StatusPiece::Percent => line_height(small.metrics()),
        };
        // Each piece sits in the middle of the widget's height, as the row of widgets it replaces placed them.
        let top = inner.y + Align::Center.offset(inner.height, piece_height);
        match piece {
            StatusPiece::Icon { library_icon, side } => {
                let reference = ImageReference::SharedLibrary(library_icon).to_value();
                let square = Rect {
                    x,
                    y: top,
                    width: side,
                    height: side,
                };
                draw_icon_in(context, reference, square, icon_color, renderer);
            }
            StatusPiece::Bolt => {
                let reference =
                    ImageReference::SharedLibrary(battletime_runtime::resources::library_icon::CHARGING);
                let square = Rect {
                    x,
                    y: top,
                    width: BOLT_SIZE,
                    height: BOLT_SIZE,
                };
                draw_icon_in(context, reference.to_value(), square, bolt_color, renderer);
            }
            StatusPiece::Battery => {
                let body = Rect {
                    x,
                    y: top,
                    width: BATTERY_WIDTH,
                    height: BATTERY_HEIGHT,
                };
                let outline = Style {
                    bg_color: TRANSPARENT,
                    border_color: battery_color,
                    border_width: BATTERY_BORDER as u8,
                    alpha: OPAQUE,
                    radius: BATTERY_RADIUS as u8,
                };
                draw_widget_bg(renderer, body, &outline);
                let fill = Rect {
                    x: x + BATTERY_BORDER,
                    y: top + BATTERY_BORDER,
                    width: (BATTERY_FILL_WIDTH * percent * 2 + 100) / 200,
                    height: BATTERY_HEIGHT - 2 * BATTERY_BORDER,
                };
                if fill.width > 0 {
                    renderer.fill_rect(fill, fill_color);
                }
            }
            StatusPiece::Percent => {
                let area = Rect {
                    x,
                    y: top,
                    width,
                    height: line_height(small.metrics()),
                };
                draw_text_block(
                    renderer,
                    small,
                    &percent_text(percent),
                    area,
                    Align::Start,
                    false,
                    battery_color,
                );
            }
        }
        x += width;
        previous = Some(piece);
    }
}

/// How big a Badge's icon and text are together.
fn badge_size(node: &Node) -> (i32, i32) {
    let Extra::Badge { icon_size, .. } = *node.extra else {
        return (0, 0);
    };
    let side = icon_size as i32;
    let font = node.settings.font.metrics();
    let text = text_width(font, &node.text);
    let gap = if text > 0 { i32::from(node.settings.gap) } else { 0 };
    (side + gap + text, side.max(line_height(font)))
}

/// An icon, then its text, each in the middle of the widget's height, together placed across by `align`.
fn draw_badge(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let Extra::Badge {
        reference,
        recolor,
        icon_size,
    } = *node.extra
    else {
        return;
    };
    let theme = context.theme;
    let inner = inset(node.bounds(), i32::from(node.settings.padding));
    let (width, _) = badge_size(node);
    let side = icon_size as i32;
    let text_color = resolve(node.settings.color, ThemeColor::Text, theme);
    let icon_color = resolve(recolor.or(node.settings.color), ThemeColor::Text, theme);
    let left = inner.x
        + node
            .settings
            .align
            .unwrap_or(Align::Start)
            .offset(inner.width, width);
    if let Some(reference) = reference {
        let top = inner.y + Align::Center.offset(inner.height, side);
        let square = Rect {
            x: left,
            y: top,
            width: side,
            height: side,
        };
        draw_icon_in(context, reference, square, icon_color, renderer);
    }
    let text_left = left + side + i32::from(node.settings.gap);
    let area = Rect {
        x: text_left,
        y: inner.y,
        width: (inner.x + inner.width - text_left).max(0),
        height: inner.height,
    };
    draw_text_block(
        renderer,
        node.settings.font,
        &node.text,
        area,
        Align::Start,
        false,
        text_color,
    );
}

// ============================================================================
// App lists
// ============================================================================

/// How tall each row of an AppList is.
pub const APP_ROW_HEIGHT: i32 = 52;
const APP_ICON_SIZE: i32 = 32;
const APP_ROW_PADDING: i32 = 8;
const APP_ICON_GAP: i32 = 12;
/// How tall a group's heading is in an AppList with sections.
pub const APP_HEADING_HEIGHT: i32 = 32;
/// The largest icon a grid cell draws, so wide cells keep room for the name.
const GRID_ICON_SIZE: i32 = 48;

/// Marks a line of an AppList's text as a group's heading rather than an app's name.
pub const APP_HEADING_MARK: char = '\u{1}';

/// How many apps an AppList holds: the host keeps the count in `value` and the names in its text.
fn app_count(node: &Node) -> i32 {
    node.settings.value.max(0)
}

/// The place in `installed_apps` of an AppList's first app.
fn first_app(node: &Node) -> usize {
    match *node.extra {
        Extra::AppList {
            first,
            sections: false,
            ..
        } => first as usize,
        _ => 0,
    }
}

/// One thing an AppList draws: an app's cell, with its place in `installed_apps`, or a group's heading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppListItem<'text> {
    pub place: Option<usize>,
    pub name: &'text str,
    pub area: Rect,
}

/// Hands `visit` every item of an AppList where it lies on the screen, scrolled, in or out of sight,
/// until `visit` returns false.
///
/// With one column each app is a row; with more they sit in square cells, and a heading always
/// takes a whole row of its own. One walk, not an iterator, so its code is in the firmware once.
///
/// `bar_room` is the width kept on the right for a scroll bar.
fn walk_app_list<'node>(node: &'node Node, bar_room: i32, visit: &mut dyn FnMut(AppListItem<'node>) -> bool) {
    let padding = i32::from(node.settings.padding);
    let gap = i32::from(node.settings.gap);
    let columns = i32::from(node.settings.columns).max(1);
    let inner_width = (node.bounds().width - 2 * padding - bar_room).max(0);
    let cell_width = ((inner_width - (columns - 1) * gap) / columns).max(0);
    let cell_height = if columns == 1 { APP_ROW_HEIGHT } else { cell_width };
    let left = node.bounds().x + padding;
    let mut top = node.bounds().y + padding - i32::from(node.scroll_offset);
    let mut column = 0;
    let mut place = first_app(node);
    let mut apps_left = app_count(node);
    for line in node.text.split('\n').filter(|line| !line.is_empty()) {
        let item = match line.strip_prefix(APP_HEADING_MARK) {
            Some(heading) => {
                if column > 0 {
                    top += cell_height + gap;
                    column = 0;
                }
                let area = Rect {
                    x: left,
                    y: top,
                    width: inner_width,
                    height: APP_HEADING_HEIGHT,
                };
                top += APP_HEADING_HEIGHT + gap;
                AppListItem {
                    place: None,
                    name: heading,
                    area,
                }
            }
            None if apps_left == 0 => return,
            None => {
                apps_left -= 1;
                let area = Rect {
                    x: left + column * (cell_width + gap),
                    y: top,
                    width: cell_width,
                    height: cell_height,
                };
                column += 1;
                if column == columns {
                    top += cell_height + gap;
                    column = 0;
                }
                place += 1;
                AppListItem {
                    place: Some(place - 1),
                    name: line,
                    area,
                }
            }
        };
        if !visit(item) {
            return;
        }
    }
}

/// The width an AppList keeps for its scroll bar: none unless layout gave it one.
fn app_list_bar_room(node: &Node) -> i32 {
    match node.shows_scroll_bar {
        true => SCROLL_BAR_ROOM,
        false => 0,
    }
}

/// How tall an AppList's items are together, with its padding, to bound how far it scrolls.
pub fn app_list_height(node: &Node) -> i32 {
    app_list_height_with_room(node, app_list_bar_room(node))
}

/// How tall an AppList's items are at its full width, which decides whether it needs a scroll bar.
///
/// Narrower grid cells are shorter, so deciding at the width the bar leaves could turn it off again.
pub fn app_list_height_without_bar(node: &Node) -> i32 {
    app_list_height_with_room(node, 0)
}

fn app_list_height_with_room(node: &Node, bar_room: i32) -> i32 {
    let padding = i32::from(node.settings.padding);
    let content_top = node.bounds().y + padding - i32::from(node.scroll_offset);
    let mut bottom = None;
    walk_app_list(node, bar_room, &mut |item| {
        bottom = Some(item.area.y + item.area.height);
        true
    });
    bottom.map_or(0, |bottom| bottom - content_top) + 2 * padding
}

/// The place of the app whose cell is under a touch, or none on a heading, between cells and past the last.
pub fn app_row_at(node: &Node, x: i32, y: i32) -> Option<i32> {
    let mut touched = None;
    walk_app_list(node, app_list_bar_room(node), &mut |item| {
        let area = item.area;
        let inside = x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height;
        if inside {
            touched = item.place.map(|place| place as i32);
        }
        !inside
    });
    touched
}

/// Each item of an AppList that is at least partly inside it.
pub fn app_list_rows(node: &Node) -> Vec<AppListItem<'_>> {
    let mut rows = Vec::new();
    walk_app_list(node, app_list_bar_room(node), &mut |item| {
        let area = item.area;
        if area.y + area.height > node.bounds().y && area.y < node.bounds().y + node.bounds().height {
            rows.push(item);
        }
        area.y < node.bounds().y + node.bounds().height
    });
    rows
}

/// Each installed app as a cell like a button, and each group's heading as text above its apps.
fn draw_app_list(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let theme = context.theme;
    let color = resolve(node.settings.color, ThemeColor::Text, theme);
    let surface = resolve(None, ThemeColor::Surface, theme);
    let in_grid = node.settings.columns > 1;
    for item in app_list_rows(node) {
        let Some(place) = item.place else {
            let heading_color = resolve(None, ThemeColor::Accent, theme);
            draw_text_block(
                renderer,
                FontName::Normal,
                item.name,
                item.area,
                Align::Start,
                false,
                heading_color,
            );
            continue;
        };
        let style = frame_style(&Settings::default(), surface, BUTTON_RADIUS, theme);
        draw_widget_bg(renderer, item.area, &style);
        let icon = ImageReference::InstalledApp(place as u16).to_value();
        if in_grid {
            draw_grid_cell(context, icon, &item, color, renderer);
        } else {
            draw_list_row(context, node, icon, &item, color, renderer);
        }
    }
}

/// An app as a row: its icon at the left, then its name.
fn draw_list_row(
    context: &DrawContext,
    node: &Node,
    icon: i32,
    item: &AppListItem,
    color: Color,
    renderer: &mut dyn Renderer,
) {
    let row = item.area;
    let icon_area = Rect {
        x: row.x + APP_ROW_PADDING,
        y: row.y + (APP_ROW_HEIGHT - APP_ICON_SIZE) / 2,
        width: APP_ICON_SIZE,
        height: APP_ICON_SIZE,
    };
    draw_image(context, icon, icon_area, color, renderer);
    let text_left = icon_area.x + APP_ICON_SIZE + APP_ICON_GAP;
    let text_area = Rect {
        x: text_left,
        y: row.y,
        width: (row.x + row.width - APP_ROW_PADDING - text_left).max(0),
        height: APP_ROW_HEIGHT,
    };
    draw_text_block(
        renderer,
        node.settings.font,
        item.name,
        text_area,
        Align::Start,
        false,
        color,
    );
}

/// An app as a square cell: a large icon, with its name in small text below when the cell has room.
fn draw_grid_cell(
    context: &DrawContext,
    icon: i32,
    item: &AppListItem,
    color: Color,
    renderer: &mut dyn Renderer,
) {
    let cell = item.area;
    let name_height = line_height(FontName::Small.metrics());
    let shows_name = cell.height >= GRID_ICON_SIZE / 2 + name_height + 2 * APP_ROW_PADDING;
    let room = cell.height - 2 * APP_ROW_PADDING - if shows_name { name_height } else { 0 };
    let side = room
        .min(cell.width - 2 * APP_ROW_PADDING)
        .clamp(0, GRID_ICON_SIZE);
    let used = side + if shows_name { name_height } else { 0 };
    let icon_area = Rect {
        x: cell.x + Align::Center.offset(cell.width, side),
        y: cell.y + Align::Center.offset(cell.height, used),
        width: side,
        height: side,
    };
    draw_image(context, icon, icon_area, color, renderer);
    if shows_name {
        let name_area = Rect {
            x: cell.x + 2,
            y: icon_area.y + side,
            width: (cell.width - 4).max(0),
            height: name_height,
        };
        let name = fitted(FontName::Small, item.name, name_area.width);
        draw_text_block(
            renderer,
            FontName::Small,
            name,
            name_area,
            Align::Center,
            false,
            color,
        );
    }
}

/// As much of the start of a text as fits a width, cut between characters.
fn fitted(font: FontName, text: &str, width: i32) -> &str {
    let metrics = font.metrics();
    let mut end = text.len();
    while end > 0 && text_width(metrics, &text[..end]) > width {
        end = text[..end].char_indices().last().map_or(0, |(index, _)| index);
    }
    &text[..end]
}

/// Where a CellGrid's cells are: the first cell's corner, the pitch from one to the next, and the size.
pub struct CellLayout {
    pub left: i32,
    pub top: i32,
    pub columns: i32,
    pub rows: i32,
    pub cell_size: i32,
    pub pitch: i32,
}

pub fn cell_layout(node: &Node) -> Option<CellLayout> {
    let Extra::CellGrid {
        rows,
        cell_size,
        cell_gap,
        ..
    } = *node.extra
    else {
        return None;
    };
    let columns = (node.settings.columns as i32).max(1);
    let rows = (rows as i32).max(1);
    let gap = cell_gap as i32;
    let area = inset(node.bounds(), i32::from(node.settings.padding));
    let fitted = ((area.width - (columns - 1) * gap) / columns).min((area.height - (rows - 1) * gap) / rows);
    let cell_size = match cell_size {
        0 => fitted.max(1),
        size => size as i32,
    };
    Some(CellLayout {
        left: area.x,
        top: area.y,
        columns,
        rows,
        cell_size,
        pitch: cell_size + gap,
    })
}

/// The cell under a point, counted across rows, if the point is on a cell.
pub fn cell_at(node: &Node, x: i32, y: i32) -> Option<i32> {
    let cells = cell_layout(node)?;
    let column = (x - cells.left).div_euclid(cells.pitch);
    let row = (y - cells.top).div_euclid(cells.pitch);
    let inside = (0..cells.columns).contains(&column) && (0..cells.rows).contains(&row);
    inside.then_some(row * cells.columns + column)
}

/// A BitmapFile's picture, each pixel the largest whole square that fits, centred, with runs of set pixels drawn as one rectangle.
fn draw_bitmap_file(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let Some(ShownBitmap { bitmap, .. }) = context
        .bitmap_files
        .iter()
        .find(|shown| shown.widget_index == node.widget_index)
    else {
        return;
    };
    let Some(&[width, height]) = bitmap.get(..2) else {
        return;
    };
    let (width, height) = (i32::from(width), i32::from(height));
    let row_bytes = (width as usize).div_ceil(8);
    let area = inset(node.bounds(), i32::from(node.settings.padding));
    let scale = (area.width / width.max(1)).min(area.height / height.max(1));
    if scale < 1 {
        return;
    }
    let left = area.x + (area.width - width * scale) / 2;
    let top = area.y + (area.height - height * scale) / 2;
    let color = resolve(node.settings.color, ThemeColor::Text, context.theme);
    let is_set = |row: &[u8], column: i32| row[column as usize / 8] & (0x80 >> (column % 8)) != 0;

    let first_row = ((context.clip.y - top) / scale).max(0);
    let end_row = (context.clip.y + context.clip.height - top + scale - 1) / scale;
    for row_number in first_row..end_row.min(height) {
        let start = 2 + row_number as usize * row_bytes;
        let Some(row) = bitmap.get(start..start + row_bytes) else {
            return;
        };
        let mut column = 0;
        while column < width {
            if !is_set(row, column) {
                column += 1;
                continue;
            }
            let run_start = column;
            while column < width && is_set(row, column) {
                column += 1;
            }
            let place = Rect {
                x: left + run_start * scale,
                y: top + row_number * scale,
                width: (column - run_start) * scale,
                height: scale,
            };
            renderer.fill_rect(place, color);
        }
    }
}

fn draw_cell_grid(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let Some(cells) = cell_layout(node) else {
        return;
    };
    let Extra::CellGrid {
        cell_shape,
        palette,
        show_numbers,
        number_map,
        number_colors,
        ..
    } = *node.extra
    else {
        return;
    };
    let number_map = context.array(number_map);
    let number_colors = context.array(number_colors);
    let number_fallback = resolve(node.settings.color, ThemeColor::Text, context.theme);
    let Some((cell_array, cell_count)) = context.array(node.settings.array) else {
        return;
    };
    let palette = context.array(palette);
    let fallback = resolve(node.settings.color, ThemeColor::Accent, context.theme);

    for cell in 0..(cells.columns * cells.rows).min(cell_count as i32) {
        let value = context.element(cell_array, cell as usize);
        let color = match palette {
            Some((palette, length)) => {
                let index = value.clamp(0, length as Value - 1) as usize;
                ColorValue::from_value(context.element(palette, index))
                    .map_or(TRANSPARENT, |color| to_color(color.resolve(context.theme)))
            }
            None if value != 0 => fallback,
            None => TRANSPARENT,
        };
        let place = Rect {
            x: cells.left + (cell % cells.columns) * cells.pitch,
            y: cells.top + (cell / cells.columns) * cells.pitch,
            width: cells.cell_size,
            height: cells.cell_size,
        };
        if color.3 != 0 {
            match cell_shape {
                0 => renderer.fill_rect(place, color),
                1 => fill_rounded_rect(renderer, place, color, to_u8(cells.cell_size / 4)),
                _ => renderer.fill_disc_aa(centre_of(place), cells.cell_size as f32 / 2.0, color),
            }
        }
        if !show_numbers {
            continue;
        }
        let number = by_value(context, number_map, value).unwrap_or(value);
        if number == 0 {
            continue;
        }
        let number_color = by_value(context, number_colors, value)
            .and_then(ColorValue::from_value)
            .map_or(number_fallback, |color| to_color(color.resolve(context.theme)));
        draw_cell_number(renderer, node.settings.font, number, place, number_color);
    }
}

// ============================================================================
// Text lines
// ============================================================================

/// The lines a TextLines shows, each with its place: the last lines of its text that fit, wrapped to its width.
pub fn text_lines_shown(node: &Node, context: &DrawContext) -> Vec<(String, Rect)> {
    let Some((codes, length)) = context.array(node.settings.array) else {
        return Vec::new();
    };
    let count = (node.settings.value.max(0) as usize).min(length);
    let text: String = (0..count)
        .filter_map(|place| {
            u32::try_from(context.element(codes, place))
                .ok()
                .and_then(char::from_u32)
        })
        .collect();
    let font = node.settings.font;
    let area = inset(node.bounds(), i32::from(node.settings.padding));
    let mut lines: Vec<&str> = WrappedLines::new(&text, font, area.width).collect();
    // A new line at the very end starts a line of its own, still empty.
    if text.is_empty() || text.ends_with('\n') {
        lines.push("");
    }
    let height = line_height(font.metrics()).max(1);
    let fitting = (area.height / height).max(0) as usize;
    let first = lines.len().saturating_sub(fitting);
    lines[first..]
        .iter()
        .enumerate()
        .map(|(place, line)| {
            let top = area.y + place as i32 * height;
            (
                String::from(*line),
                Rect {
                    x: area.x,
                    y: top,
                    width: area.width,
                    height,
                },
            )
        })
        .collect()
}

/// The element of an array, such as a CellGrid's number map, for a cell value; the last for values past its end.
fn by_value(context: &DrawContext, array: Option<(u8, usize)>, value: Value) -> Option<Value> {
    array.map(|(array, length)| context.element(array, value.clamp(0, length as Value - 1) as usize))
}

/// The numbers a CellGrid shows, each with its cell, in drawing order.
pub fn cell_grid_numbers(node: &Node, context: &DrawContext) -> Vec<(Value, Rect)> {
    let (
        Some(cells),
        Extra::CellGrid {
            show_numbers: true,
            number_map,
            ..
        },
    ) = (cell_layout(node), *node.extra)
    else {
        return Vec::new();
    };
    let Some((cell_array, cell_count)) = context.array(node.settings.array) else {
        return Vec::new();
    };
    let number_map = context.array(number_map);
    (0..(cells.columns * cells.rows).min(cell_count as i32))
        .filter_map(|cell| {
            let value = context.element(cell_array, cell as usize);
            let number = by_value(context, number_map, value).unwrap_or(value);
            let place = Rect {
                x: cells.left + (cell % cells.columns) * cells.pitch,
                y: cells.top + (cell / cells.columns) * cells.pitch,
                width: cells.cell_size,
                height: cells.cell_size,
            };
            (number != 0).then_some((number, place))
        })
        .collect()
}

/// Draws a number centred in a cell, in a smaller font when it is too wide for the one asked for.
fn draw_cell_number(renderer: &mut dyn Renderer, font: FontName, number: Value, place: Rect, color: Color) {
    let mut digits = [0u8; 20];
    let mut writer = battletime_runtime::text::TextWriter::new(&mut digits, 0);
    if writer.write_number(number, 1).is_err() {
        return;
    }
    let length = writer.position();
    let Ok(text) = core::str::from_utf8(&digits[..length]) else {
        return;
    };
    let fits = |font: FontName| text_width(font.metrics(), text) <= place.width;
    let font = [font, FontName::Small]
        .into_iter()
        .find(|&font| fits(font))
        .unwrap_or(FontName::Small);
    draw_text_block(renderer, font, text, place, Align::Center, false, color);
}

// ============================================================================
// Text files
// ============================================================================

/// How many lines a TextFile's file wraps into, and the first of those its text holds.
fn text_file_lines(node: &Node) -> (i32, i32) {
    match *node.extra {
        Extra::TextFile {
            line_count,
            first_line,
            ..
        } => (i32::from(line_count), i32::from(first_line)),
        _ => (0, 0),
    }
}

/// How tall a TextFile's whole file is with its padding, to bound how far it scrolls.
pub fn text_file_height(node: &Node) -> i32 {
    let (line_count, _) = text_file_lines(node);
    line_count * line_height(node.settings.font.metrics()) + 2 * i32::from(node.settings.padding)
}

/// How wide a TextFile wraps its lines: inside its padding, less the room for a scroll bar when it shows one.
pub fn text_file_width(inner_width: i32, shows_scroll_bar: bool) -> i32 {
    match shows_scroll_bar {
        true => (inner_width - SCROLL_BAR_ROOM).max(1),
        false => inner_width,
    }
}

/// The lines a TextFile shows at its scroll offset, as the first and one past the last.
pub fn text_file_lines_in_view(node: &Node) -> (i32, i32) {
    let height = line_height(node.settings.font.metrics()).max(1);
    let top = i32::from(node.scroll_offset) - i32::from(node.settings.padding);
    let first = (top / height).max(0);
    let end = (top + node.bounds().height + height - 1) / height + 1;
    (first, end.max(first))
}

/// Each line a TextFile holds that is at least partly inside it, with its top.
pub fn text_file_rows(node: &Node) -> impl Iterator<Item = (&str, i32)> + '_ {
    let (_, first_line) = text_file_lines(node);
    let height = line_height(node.settings.font.metrics());
    let top = node.bounds().y + i32::from(node.settings.padding) - i32::from(node.scroll_offset);
    node.text
        .split('\n')
        .enumerate()
        .filter(|_| !node.text.is_empty())
        .map(move |(place, line)| (line, top + (first_line + place as i32) * height))
        .filter(move |(_, line_top)| {
            *line_top + height > node.bounds().y && *line_top < node.bounds().y + node.bounds().height
        })
}

/// The lines of a text file in view, each from the left edge inside the padding.
fn draw_text_file(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let font = node.settings.font;
    let ascent = font.metrics().line_metrics().ascent as i32;
    let is_placeholder = matches!(
        *node.extra,
        Extra::TextFile {
            is_placeholder: true,
            ..
        }
    );
    let color = match is_placeholder {
        true => resolve(None, ThemeColor::Muted, context.theme),
        false => resolve(node.settings.color, ThemeColor::Text, context.theme),
    };
    let left = node.bounds().x + i32::from(node.settings.padding);
    let height = line_height(font.metrics());
    let painted = |top: i32| top + height > context.clip.y && top < context.clip.y + context.clip.height;
    for (line, top) in text_file_rows(node).filter(|(_, top)| painted(*top)) {
        draw_line(renderer, font, line, left, top + ascent, color);
    }
}

// ============================================================================
// Scroll bars
// ============================================================================

/// How thick a scroll bar is.
pub const SCROLL_BAR_WIDTH: i32 = 2;
/// How much room a widget keeps beside its content for its scroll bar: the bar and a gap.
pub const SCROLL_BAR_ROOM: i32 = 4;
/// Pixels left clear at each end of the bar, and of each page's segment so the segments stand apart.
pub const SCROLL_BAR_INSET: i32 = 2;
/// The shortest a smooth scroll bar's thumb gets, however long the content.
const SMALLEST_THUMB: i32 = 12;

/// The strip a widget's scroll bar runs along: its right edge, or its bottom edge when it scrolls across.
pub fn scroll_bar_track(node: &Node) -> Rect {
    let bounds = node.bounds();
    match node.runs_across() {
        true => Rect {
            y: bounds.y + bounds.height - SCROLL_BAR_WIDTH,
            height: SCROLL_BAR_WIDTH,
            ..bounds
        },
        false => Rect {
            x: bounds.x + bounds.width - SCROLL_BAR_WIDTH,
            width: SCROLL_BAR_WIDTH,
            ..bounds
        },
    }
}

/// How many pages a widget's content makes, and which one it shows: the last page ends with the content.
pub fn scroll_pages(node: &Node, content_length: i32, offset: i32) -> (i32, i32) {
    let page = visible_length(node).max(1);
    let farthest = (content_length - page).max(0);
    let pages = 1 + (farthest + page - 1) / page;
    let current = match offset >= farthest {
        true => pages - 1,
        false => (offset + page / 2) / page,
    };
    (pages, current.clamp(0, pages - 1))
}

/// Gives each part of a scroll bar, along the track, and whether it is the bright part that marks the view.
///
/// Paged scrolling has a segment per page, as wasp-os draws it. Smooth scrolling has a thumb as
/// long as the share of the content in view, over a dim track.
pub fn for_each_scroll_bar_part(node: &Node, content_length: i32, part: impl FnMut(Rect, bool)) {
    for_each_scroll_bar_part_at(node, content_length, i32::from(node.scroll_offset), part);
}

/// Where a smooth scroll bar's thumb lies when its widget is scrolled to `offset`; none for a paged bar.
pub fn scroll_bar_thumb_at(node: &Node, content_length: i32, offset: i32) -> Option<Rect> {
    let mut thumb = None;
    for_each_scroll_bar_part_at(node, content_length, offset, |area, is_bright| {
        if is_bright && !node.scrolls_by_pages() {
            thumb = Some(area);
        }
    });
    thumb
}

/// Gives each part of a scroll bar as it is with its widget scrolled to `offset`.
fn for_each_scroll_bar_part_at(
    node: &Node,
    content_length: i32,
    offset: i32,
    mut part: impl FnMut(Rect, bool),
) {
    let track = scroll_bar_track(node);
    let across = node.runs_across();
    let length = if across { track.width } else { track.height };
    let along = |start: i32, end: i32| match across {
        true => Rect {
            x: track.x + start,
            width: end - start,
            ..track
        },
        false => Rect {
            y: track.y + start,
            height: end - start,
            ..track
        },
    };
    let (pages, current) = scroll_pages(node, content_length, offset);
    let pitch = length / pages.max(1);
    if node.scrolls_by_pages() && pitch - 2 * SCROLL_BAR_INSET >= SCROLL_BAR_WIDTH {
        for page in 0..pages {
            part(
                along(
                    page * pitch + SCROLL_BAR_INSET,
                    (page + 1) * pitch - SCROLL_BAR_INSET,
                ),
                page == current,
            );
        }
        return;
    }
    let room = (length - 2 * SCROLL_BAR_INSET).max(0);
    let visible = visible_length(node);
    let farthest = (content_length - visible).max(1);
    let thumb = (room * visible / content_length.max(1)).clamp(SMALLEST_THUMB.min(room), room);
    let start = SCROLL_BAR_INSET + (room - thumb) * offset.clamp(0, farthest) / farthest;
    part(along(SCROLL_BAR_INSET, SCROLL_BAR_INSET + room), false);
    part(along(start, start + thumb), true);
}

/// Draws a widget's scroll bar over whatever lies under it: the view's part bright, the rest dim.
pub fn draw_scroll_bar(node: &Node, content_length: i32, context: &DrawContext, renderer: &mut dyn Renderer) {
    let bright = resolve(None, ThemeColor::Muted, context.theme);
    let dim = resolve(None, ThemeColor::Surface, context.theme);
    for_each_scroll_bar_part(node, content_length, |area, is_bright| {
        if let Some(area) = area.intersect(context.clip) {
            renderer.fill_rect(area, if is_bright { bright } else { dim });
        }
    });
}

/// Gives each part of a Pages' bar along its bottom: a segment per page with the one on show bright,
/// or, when the segments would be too short, a bright thumb over a dim track.
pub fn for_each_page_bar_part(node: &Node, mut part: impl FnMut(Rect, bool)) {
    let bounds = node.bounds();
    let pages = i32::from(node.option_count).max(1);
    let current = node.settings.value.clamp(0, pages - 1);
    let along = |start: i32, end: i32| Rect {
        x: bounds.x + start,
        y: bounds.y + bounds.height - SCROLL_BAR_WIDTH,
        width: end - start,
        height: SCROLL_BAR_WIDTH,
    };
    let pitch = bounds.width / pages;
    if pitch - 2 * SCROLL_BAR_INSET >= SCROLL_BAR_WIDTH {
        for page in 0..pages {
            part(
                along(
                    page * pitch + SCROLL_BAR_INSET,
                    (page + 1) * pitch - SCROLL_BAR_INSET,
                ),
                page == current,
            );
        }
        return;
    }
    let room = (bounds.width - 2 * SCROLL_BAR_INSET).max(0);
    let thumb = (room / pages).max(SMALLEST_THUMB.min(room));
    let start = SCROLL_BAR_INSET + (room - thumb) * current / (pages - 1).max(1);
    part(along(SCROLL_BAR_INSET, SCROLL_BAR_INSET + room), false);
    part(along(start, start + thumb), true);
}

/// Draws a Pages' bar along its bottom, in the colours of a scroll bar.
fn draw_page_bar(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let bright = resolve(None, ThemeColor::Muted, context.theme);
    let dim = resolve(None, ThemeColor::Surface, context.theme);
    for_each_page_bar_part(node, |area, is_bright| {
        if let Some(area) = area.intersect(context.clip) {
            renderer.fill_rect(area, if is_bright { bright } else { dim });
        }
    });
}

// ============================================================================
// Lines and charts
// ============================================================================

/// The size a Line's points reach to, from its top-left corner.
pub fn points_extent(context: &DrawContext, variable: Option<u8>) -> (i32, i32) {
    let Some((points, length)) = context.array(variable) else {
        return (0, 0);
    };
    let mut extent = (0, 0);
    for pair in 0..length / 2 {
        let x = context.element(points, 2 * pair) as i32;
        let y = context.element(points, 2 * pair + 1) as i32;
        extent = (extent.0.max(x + 1), extent.1.max(y + 1));
    }
    extent
}

fn draw_polyline(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let Some((points, length)) = context.array(node.settings.array) else {
        return;
    };
    let color = resolve(node.settings.color, ThemeColor::Text, context.theme);
    let width = node
        .settings
        .border_width
        .get()
        .map_or(LINE_WIDTH, |width| width.max(1) as f32);
    let point = |pair: usize| {
        PointF::new(
            node.bounds().x as f32 + context.element(points, 2 * pair) as f32,
            node.bounds().y as f32 + context.element(points, 2 * pair + 1) as f32,
        )
    };
    for pair in 1..length / 2 {
        renderer.stroke_line_aa(point(pair - 1), point(pair), width, color);
        if width > 2.0 && pair + 1 < length / 2 {
            renderer.fill_disc_aa(point(pair), width / 2.0, color);
        }
    }
}

fn draw_chart(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let Some((values, length)) = context.array(node.settings.array) else {
        return;
    };
    let settings = &node.settings;
    let area = inset(node.bounds(), i32::from(settings.padding).max(1));
    let color = resolve(settings.color, ThemeColor::Accent, context.theme);
    let span = (settings.maximum as f32 - settings.minimum as f32).max(1.0);
    let height = (area.height - 1) as f32;
    let point = |index: usize| {
        let value =
            (context.element(values, index) as f32).clamp(settings.minimum as f32, settings.maximum as f32);
        let across = match length {
            1 => 0.0,
            _ => index as f32 * (area.width - 1) as f32 / (length - 1) as f32,
        };
        PointF::new(
            area.x as f32 + across,
            area.y as f32 + height - (value - settings.minimum as f32) * height / span,
        )
    };
    for index in 1..length {
        renderer.stroke_line_aa(point(index - 1), point(index), CHART_LINE_WIDTH, color);
        renderer.fill_disc_aa(point(index), CHART_LINE_WIDTH / 2.0, color);
    }
}

// ============================================================================
// Scales and clocks
// ============================================================================

/// The angle of a tick: spread round a whole circle, or from the start to the end of a part dial.
fn tick_angle(start: f32, span: f32, tick: u16, ticks: u16) -> f32 {
    let steps = match span >= 360.0 {
        true => ticks as f32,
        false => (ticks.max(2) - 1) as f32,
    };
    start + span * tick as f32 / steps
}

fn dial_angles(settings: &Settings) -> (f32, f32) {
    let start = settings.start_angle.get().map(i32::from).unwrap_or(0) as f32;
    let end = settings.end_angle.get().map(i32::from).unwrap_or(360) as f32;
    let mut span = end - start;
    if span <= 0.0 {
        span += 360.0;
    }
    (start, span)
}

fn draw_scale(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let Extra::Scale {
        ticks,
        major_every,
        skip,
        labels,
        needle_color,
        major_color,
    } = *node.extra
    else {
        return;
    };
    let settings = &node.settings;
    let theme = context.theme;
    let area = inset(node.bounds(), i32::from(settings.padding));
    let centre = centre_of(area);
    let radius = area.width.min(area.height) as f32 / 2.0;
    let (start, span) = dial_angles(settings);
    let minor_color = resolve(settings.color, ThemeColor::Muted, theme);
    let major_color = resolve(major_color, ThemeColor::Text, theme);
    let skipped = context.array(skip);
    let labels = context.array(labels);
    let is_skipped = |tick: u16| {
        skipped.is_some_and(|(variable, length)| {
            (0..length).any(|index| context.element(variable, index) == tick as Value)
        })
    };

    for tick in 0..ticks {
        if is_skipped(tick) {
            continue;
        }
        let is_major = major_every > 0 && tick % major_every == 0;
        let angle = tick_angle(start, span, tick, ticks);
        let (length, width, color) = match is_major {
            true => (SCALE_MAJOR_TICK_LENGTH, SCALE_MAJOR_TICK_WIDTH, major_color),
            false => (SCALE_MINOR_TICK_LENGTH, SCALE_MINOR_TICK_WIDTH, minor_color),
        };
        let outer = point_on_circle(centre, radius - 1.0, angle);
        let inner = point_on_circle(centre, radius - 1.0 - length, angle);
        renderer.stroke_line_aa(inner, outer, width, color);

        let label_index = (tick / major_every.max(1)) as usize;
        if let (true, Some((variable, length))) = (is_major, labels) {
            if label_index < length {
                let text = context.text_element(variable, label_index);
                let font = settings.font;
                let metrics = font.metrics();
                let half_height = line_height(metrics) as f32 / 2.0;
                let along = radius - 1.0 - SCALE_MAJOR_TICK_LENGTH - SCALE_LABEL_SPACE - half_height;
                let middle = point_on_circle(centre, along, angle);
                let width = text_width(metrics, text);
                let ascent = metrics.line_metrics().ascent as i32;
                let left = middle.x as i32 - width / 2;
                let top = (middle.y - half_height) as i32;
                draw_line(renderer, font, text, left, top + ascent, major_color);
            }
        }
    }

    if needle_color.is_some() || settings.value != 0 {
        let fraction =
            (settings.value - settings.minimum) as f32 / (settings.maximum - settings.minimum).max(1) as f32;
        let angle = start + span * fraction.clamp(0.0, 1.0);
        let color = resolve(needle_color, ThemeColor::Accent, theme);
        let tip = point_on_circle(centre, radius - SCALE_MAJOR_TICK_LENGTH - 2.0, angle);
        round_line(renderer, centre, tip, NEEDLE_WIDTH, color);
        renderer.fill_disc_aa(centre, CLOCK_CAP_RADIUS, color);
    }
}

/// Draws one clock hand from the centre in its style: a line, a taper or a line with round ends.
fn draw_hand(
    renderer: &mut dyn Renderer,
    centre: PointF,
    length: f32,
    angle: f32,
    width: f32,
    style: u8,
    color: Color,
) {
    let tip = point_on_circle(centre, length, angle);
    match style {
        0 => renderer.stroke_line_aa(centre, tip, width, color),
        1 => {
            const TAPER_STEPS: i32 = 4;
            for step in 0..TAPER_STEPS {
                let from = point_on_circle(centre, length * step as f32 / TAPER_STEPS as f32, angle);
                let to = point_on_circle(centre, length * (step + 1) as f32 / TAPER_STEPS as f32, angle);
                let step_width = width * (TAPER_STEPS - step) as f32 / TAPER_STEPS as f32;
                renderer.stroke_line_aa(from, to, step_width.max(1.0), color);
            }
        }
        _ => round_line(renderer, centre, tip, width, color),
    }
}

const MILLISECONDS_PER_SECOND: Value = 1000;
const SECONDS_PER_DAY: Value = 86_400;

fn draw_analog_clock(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let Extra::AnalogClock {
        time,
        show_seconds,
        hand_style,
        hour_color,
        minute_color,
        second_color,
        hour_length,
        minute_length,
        second_length,
    } = *node.extra
    else {
        return;
    };
    let theme = context.theme;
    let area = inset(node.bounds(), i32::from(node.settings.padding));
    let centre = centre_of(area);
    let radius = area.width.min(area.height) as f32 / 2.0;
    let length_of = |length: Option<u16>, share: i32| {
        length.map_or(radius * share as f32 / 100.0, |length| length as f32)
    };

    let local_seconds = time
        .saturating_add(context.utc_offset)
        .div_euclid(MILLISECONDS_PER_SECOND)
        .rem_euclid(SECONDS_PER_DAY);
    let hours = (local_seconds / 3600 % 12) as f32;
    let minutes = (local_seconds / 60 % 60) as f32;
    let seconds = (local_seconds % 60) as f32;
    let hour_angle = hours * 30.0 + minutes / 2.0;
    let minute_angle = minutes * 6.0 + seconds / 10.0;
    let second_angle = seconds * 6.0;

    let hour_color = resolve(hour_color.or(node.settings.color), ThemeColor::Text, theme);
    let minute_color = minute_color.map_or(hour_color, |color| to_color(color.resolve(theme)));
    let second_color = resolve(second_color, ThemeColor::Accent, theme);
    let [hour_share, minute_share, second_share] = HAND_SHARE_PERCENT;

    draw_hand(
        renderer,
        centre,
        length_of(hour_length, hour_share),
        hour_angle,
        HOUR_HAND_WIDTH,
        hand_style,
        hour_color,
    );
    draw_hand(
        renderer,
        centre,
        length_of(minute_length, minute_share),
        minute_angle,
        MINUTE_HAND_WIDTH,
        hand_style,
        minute_color,
    );
    renderer.fill_disc_aa(centre, CLOCK_CAP_RADIUS, minute_color);
    if show_seconds {
        draw_hand(
            renderer,
            centre,
            length_of(second_length, second_share),
            second_angle,
            SECOND_HAND_WIDTH,
            2,
            second_color,
        );
        renderer.fill_disc_aa(centre, CLOCK_CAP_RADIUS / 2.0, second_color);
    }
}

// ============================================================================
// Images
// ============================================================================

fn draw_image_widget(node: &Node, context: &DrawContext, area: Rect, renderer: &mut dyn Renderer) {
    let Extra::Image { reference, recolor } = *node.extra else {
        return;
    };
    let color = resolve(recolor.or(node.settings.color), ThemeColor::Text, context.theme);
    let recolors = recolor.is_some();
    let Some(reference) = reference else {
        draw_placeholder(renderer, area, color);
        return;
    };
    match context.image_resource(reference) {
        Some(image) => {
            let (width, height) = (image.width as i32, image.height as i32);
            let (shown_width, shown_height) = if area.width * height <= area.height * width {
                (
                    area.width.max(1),
                    ((height * area.width + width / 2) / width).max(1),
                )
            } else {
                (
                    ((width * area.height + height / 2) / height).max(1),
                    area.height.max(1),
                )
            };
            let place = Rect {
                x: area.x + Align::Center.offset(area.width, shown_width),
                y: area.y + Align::Center.offset(area.height, shown_height),
                width: shown_width,
                height: shown_height,
            };
            draw_resource(&image, place, color, recolors, renderer);
        }
        None => draw_placeholder(renderer, area, color),
    }
}

/// Draws the image a value names inside `area`, centred, or a placeholder if it cannot be found.
fn draw_image(context: &DrawContext, reference: i32, area: Rect, color: Color, renderer: &mut dyn Renderer) {
    match context.image_resource(reference) {
        Some(image) => draw_resource(&image, area, color, false, renderer),
        None => draw_placeholder(renderer, area, color),
    }
}

/// A box with a cross, where an image that cannot be found would be.
fn draw_placeholder(renderer: &mut dyn Renderer, area: Rect, color: Color) {
    let side = area.width.min(area.height).clamp(4, PLACEHOLDER_SIZE);
    let square = Rect {
        x: area.x + Align::Center.offset(area.width, side),
        y: area.y + Align::Center.offset(area.height, side),
        width: side,
        height: side,
    };
    let faint = with_alpha(color, OPAQUE / 2);
    let style = Style {
        bg_color: TRANSPARENT,
        border_color: faint,
        border_width: 1,
        alpha: OPAQUE,
        radius: 2,
    };
    draw_widget_bg(renderer, square, &style);
    let corner = |x: i32, y: i32| PointF::new(x as f32, y as f32);
    let right = square.x + side;
    let bottom = square.y + side;
    renderer.stroke_line_aa(corner(square.x, square.y), corner(right, bottom), 1.0, faint);
    renderer.stroke_line_aa(corner(right, square.y), corner(square.x, bottom), 1.0, faint);
}

/// The coverage a colour image's pixel gives when it is drawn in one colour: its brightest channel.
fn brightness(rgb565: u16) -> u8 {
    let color = to_color(rgb565);
    color.0.max(color.1).max(color.2)
}

/// Draws an image's runs row by row, fitted to `place`'s size and positioned at its top-left corner.
///
/// `place` may be smaller or larger than the image itself: each source row and column maps onto the
/// destination range it divides into, so the whole image always lands inside `place` with no gaps or
/// double-drawn pixels, rather than being drawn at native size and left for the caller to clip.
pub fn draw_resource(
    image: &ImageResource,
    place: Rect,
    color: Color,
    recolors: bool,
    renderer: &mut dyn Renderer,
) {
    const LONGEST_SPAN: usize = 64;
    let width = image.width as i32;
    let height = image.height as i32;
    let scaled_x = |column: i32| column * place.width / width;
    let scaled_y = |row: i32| row * place.height / height;
    // Screen rows the renderer can show, relative to the image's top; the runs are read in order, so rows below end it.
    let (first_shown, end_shown) = fonts::rows_to_draw(renderer, place.y, place.height);
    let (first_shown_column, end_shown_column) = fonts::columns_to_draw(renderer, place.x, place.width);
    let mask_coverage_of = |value: u16| match (image.format, recolors) {
        (ImageFormat::Mask, _) => Some(value as u8),
        (ImageFormat::Color, true) => Some(brightness(value)),
        (ImageFormat::Color, false) => None,
    };
    if first_shown >= end_shown || first_shown_column >= end_shown_column {
        return;
    }
    // The first source row that reaches a shown row; the runs before it are skipped by length alone.
    let mut row = first_shown * height / place.height.max(1);
    while row > 0 && scaled_y(row) > first_shown {
        row -= 1;
    }
    while scaled_y(row + 1) <= first_shown {
        row += 1;
    }
    let first_pixel = (row * width) as u32;
    let (run_start, runs) = image.runs_from(first_pixel);
    let mut coverage = [0u8; LONGEST_SPAN];
    let mut column = 0i32;
    let (mut row_top, mut row_bottom) = (scaled_y(row), scaled_y(row + 1));
    for (index, run) in runs.enumerate() {
        // The first run may begin rows above, and only the part from the first pixel on is drawn.
        let skipped = match index {
            0 => (first_pixel - run_start) as i32,
            _ => 0,
        };
        let mut left = run.count as i32 - skipped;
        while left > 0 {
            if row_top >= end_shown {
                return;
            }
            let span = left.min(width - column);
            let (y_from, y_to) = (row_top.max(first_shown), row_bottom.min(end_shown));
            // Only the part of the run inside the shown columns is drawn.
            let (x_from, x_to) = match y_to > y_from {
                true => (
                    scaled_x(column).max(first_shown_column),
                    scaled_x(column + span).min(end_shown_column),
                ),
                false => (0, 0),
            };
            if x_to > x_from {
                let x = place.x + x_from;
                let draw_span = x_to - x_from;
                for y in place.y + y_from..place.y + y_to {
                    let line = Rect {
                        x,
                        y,
                        width: draw_span,
                        height: 1,
                    };
                    match mask_coverage_of(run.value) {
                        None => renderer.fill_rect(line, to_color(run.value)),
                        Some(0) => {}
                        Some(OPAQUE) => renderer.fill_rect(line, color),
                        Some(value) => {
                            let mut drawn = 0;
                            while drawn < draw_span {
                                let piece = (draw_span - drawn).min(LONGEST_SPAN as i32) as usize;
                                coverage[..piece].fill(value);
                                renderer.blend_row(x + drawn, y, color, &coverage[..piece]);
                                drawn += piece as i32;
                            }
                        }
                    }
                }
            }
            column += span;
            left -= span;
            if column == width {
                column = 0;
                row += 1;
                (row_top, row_bottom) = (row_bottom, scaled_y(row + 1));
            }
        }
    }
}

// ============================================================================
// Choices: button matrices, rollers and dropdowns
// ============================================================================

/// The row break a ButtonMatrix's options hold between rows.
const ROW_BREAK: &str = "\n";

/// Each button of a ButtonMatrix with its place, numbered across the rows without the breaks.
pub fn matrix_buttons<'context>(
    node: &'context Node,
    context: &'context DrawContext,
) -> impl Iterator<Item = (i32, Rect, &'context str)> + 'context {
    let options = context.array(node.settings.array);
    let length = options.map_or(0, |(_, length)| length);
    let text = move |index: usize| options.map_or("", |(variable, _)| context.text_element(variable, index));
    let row_count = (0..length).filter(|&index| text(index) == ROW_BREAK).count() as i32 + 1;
    let area = inset(node.bounds(), i32::from(node.settings.padding));
    let row_height = (area.height - (row_count - 1) * MATRIX_GAP) / row_count.max(1);

    let mut index = 0;
    let mut row = 0;
    let mut number = 0;
    core::iter::from_fn(move || {
        while index < length && text(index) == ROW_BREAK {
            index += 1;
            row += 1;
        }
        if index >= length {
            return None;
        }
        let row_start = (0..index)
            .rev()
            .find(|&before| text(before) == ROW_BREAK)
            .map_or(0, |found| found + 1);
        let row_end = (index..length)
            .find(|&after| text(after) == ROW_BREAK)
            .unwrap_or(length);
        let in_row = (row_end - row_start) as i32;
        let column = (index - row_start) as i32;
        let button_width = (area.width - (in_row - 1) * MATRIX_GAP) / in_row.max(1);
        let place = Rect {
            x: area.x + column * (button_width + MATRIX_GAP),
            y: area.y + row * (row_height + MATRIX_GAP),
            width: button_width,
            height: row_height,
        };
        let found = (number, place, text(index));
        index += 1;
        number += 1;
        Some(found)
    })
}

fn draw_button_matrix(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let settings = &node.settings;
    let theme = context.theme;
    let surface = resolve(None, ThemeColor::Surface, theme);
    let highlight = resolve(settings.background, ThemeColor::Accent, theme);
    let text_color = resolve(settings.color, ThemeColor::Text, theme);
    for (number, place, text) in matrix_buttons(node, context) {
        let is_highlighted = number < 32 && (settings.value as u32 >> number) & 1 == 1;
        let fill = if is_highlighted { highlight } else { surface };
        fill_rounded_rect(renderer, place, fill, MATRIX_RADIUS);
        draw_text_block(
            renderer,
            settings.font,
            text,
            place,
            Align::Center,
            false,
            text_color,
        );
    }
}

/// How tall one option is in a Roller or a Dropdown.
pub fn option_row_height(node: &Node) -> i32 {
    option_row_height_of_font(node.settings.font)
}

/// How tall one option is in a Roller drawn in this font.
pub fn option_row_height_of_font(font: FontName) -> i32 {
    line_height(font.metrics()) + 2 * OPTION_ROW_PADDING
}

/// How many rows a Roller shows: as many as fit, always an odd number so the chosen one is in the middle.
pub fn roller_rows(node: &Node) -> i32 {
    let fit = (node.bounds().height / option_row_height(node).max(1)).max(1);
    if fit % 2 == 0 {
        fit - 1
    } else {
        fit
    }
}

fn draw_roller(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let Some((options, length)) = context.array(node.settings.array) else {
        return;
    };
    let settings = &node.settings;
    let theme = context.theme;
    let row_height = option_row_height(node);
    let rows = roller_rows(node);
    let top = node.bounds().y + Align::Center.offset(node.bounds().height, rows * row_height);
    let chosen = settings.value;
    let band = Rect {
        x: node.bounds().x,
        y: top + rows / 2 * row_height,
        width: node.bounds().width,
        height: row_height,
    };
    fill_rounded_rect(
        renderer,
        band,
        resolve(None, ThemeColor::Surface, theme),
        MATRIX_RADIUS,
    );
    for row in 0..rows {
        let option = chosen + row - rows / 2;
        if option < 0 || option >= length as i32 {
            continue;
        }
        let is_chosen = option == chosen;
        let color = match is_chosen {
            true => resolve(settings.color, ThemeColor::Text, theme),
            false => resolve(None, ThemeColor::Muted, theme),
        };
        let place = Rect {
            y: top + row * row_height,
            height: row_height,
            ..node.bounds()
        };
        draw_text_block(
            renderer,
            settings.font,
            context.text_element(options, option as usize),
            place,
            Align::Center,
            false,
            color,
        );
    }
}

/// The area an open Dropdown's list covers: its box, then one row per option below it.
pub fn open_list_area(node: &Node) -> Rect {
    Rect {
        height: node.bounds().height + node.option_count as i32 * option_row_height(node),
        ..node.bounds()
    }
}

fn draw_dropdown(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let settings = &node.settings;
    let theme = context.theme;
    let surface = resolve(settings.background, ThemeColor::Surface, theme);
    let text_color = resolve(settings.color, ThemeColor::Text, theme);
    let style = frame_style(settings, surface, BUTTON_RADIUS / 2, theme);
    draw_widget_bg(renderer, node.bounds(), &style);

    let options = context.array(settings.array);
    let text_of = |index: i32| match options {
        Some((variable, length)) if (0..length as i32).contains(&index) => {
            context.text_element(variable, index as usize)
        }
        _ => "",
    };
    let area = inset(node.bounds(), OPTION_ROW_PADDING);
    let arrow_x = (area.x + area.width) as f32 - DROPDOWN_ARROW_SIZE;
    let arrow_y = area.y as f32 + area.height as f32 / 2.0;
    let text_area = Rect {
        width: (area.width - 3 * DROPDOWN_ARROW_SIZE as i32).max(0),
        ..area
    };
    draw_text_block(
        renderer,
        settings.font,
        text_of(settings.value),
        text_area,
        Align::Start,
        false,
        text_color,
    );
    let flip = if node.is_open { -1.0 } else { 1.0 };
    let arm = |dx: f32| PointF::new(arrow_x + dx, arrow_y - flip * DROPDOWN_ARROW_SIZE / 2.0);
    let tip = PointF::new(arrow_x, arrow_y + flip * DROPDOWN_ARROW_SIZE / 2.0);
    round_line(renderer, arm(-DROPDOWN_ARROW_SIZE), tip, 2.0, text_color);
    round_line(renderer, arm(DROPDOWN_ARROW_SIZE), tip, 2.0, text_color);

    let Some((_, length)) = options.filter(|_| node.is_open) else {
        return;
    };
    let row_height = option_row_height(node);
    let list = Rect {
        y: node.bounds().y + node.bounds().height,
        height: length as i32 * row_height,
        ..node.bounds()
    };
    fill_rounded_rect(renderer, list, surface, (BUTTON_RADIUS / 2) as u8);
    for option in 0..length as i32 {
        let row = Rect {
            y: list.y + option * row_height,
            height: row_height,
            ..list
        };
        if option == settings.value {
            fill_rounded_rect(
                renderer,
                row,
                resolve(None, ThemeColor::Accent, theme),
                (BUTTON_RADIUS / 2) as u8,
            );
        }
        let place = inset(row, OPTION_ROW_PADDING);
        draw_text_block(
            renderer,
            settings.font,
            text_of(option),
            place,
            Align::Start,
            false,
            text_color,
        );
    }
}

/// The option under a point in an open Dropdown's list.
pub fn dropdown_option_at(node: &Node, y: i32) -> Option<i32> {
    let below = y - (node.bounds().y + node.bounds().height);
    (below >= 0).then(|| below / option_row_height(node).max(1))
}

// ============================================================================
// Measuring
// ============================================================================

const MATRIX_ROW_HEIGHT: i32 = 40;

/// Works out the size a node's images or arrays want, for layout.
pub fn measure_node(node: &mut Node, context: &DrawContext) {
    use battletime_runtime::WidgetKind::*;
    let options = context.array(node.settings.array);
    let widest_option = || {
        options.map_or(0, |(variable, length)| {
            (0..length)
                .map(|index| {
                    text_width(
                        node.settings.font.metrics(),
                        context.text_element(variable, index),
                    )
                })
                .max()
                .unwrap_or(0)
        })
    };
    let (width, height) = match node.kind {
        Image | Icon | Button => match *node.extra {
            Extra::Image {
                reference: Some(reference),
                ..
            } => context
                .image_size(reference)
                .unwrap_or((PLACEHOLDER_SIZE, PLACEHOLDER_SIZE)),
            _ if node.kind == Button => (0, 0),
            _ => (PLACEHOLDER_SIZE, PLACEHOLDER_SIZE),
        },
        CellGrid => match cell_layout(node) {
            Some(cells) if matches!(*node.extra, Extra::CellGrid { cell_size, .. } if cell_size > 0) => (
                cells.columns * cells.pitch - (cells.pitch - cells.cell_size),
                cells.rows * cells.pitch - (cells.pitch - cells.cell_size),
            ),
            _ => (0, 0),
        },
        Line => points_extent(context, node.settings.array),
        Battery | StatusIcons => {
            let pieces = status_pieces(node);
            (pieces.width(), pieces.height)
        }
        Badge => badge_size(node),
        TimeRollers | DurationRollers | NumberRoller => pickers::rollers_size(node),
        WeekdayPicker => pickers::weekdays_size(node),
        Stepper => pickers::stepper_size(node),
        Confirm => (0, 0),
        Roller => (
            widest_option() + 4 * OPTION_ROW_PADDING,
            3 * option_row_height(node),
        ),
        Dropdown => {
            node.option_count = options.map_or(0, |(_, length)| length.min(u8::MAX as usize) as u8);
            (
                widest_option() + 2 * OPTION_ROW_PADDING + 3 * DROPDOWN_ARROW_SIZE as i32,
                option_row_height(node),
            )
        }
        ButtonMatrix => {
            let breaks = options.map_or(0, |(variable, length)| {
                (0..length)
                    .filter(|&index| context.text_element(variable, index) == ROW_BREAK)
                    .count() as i32
            });
            (0, (breaks + 1) * MATRIX_ROW_HEIGHT + breaks * MATRIX_GAP)
        }
        _ => (0, 0),
    };
    node.content_width = width.clamp(0, i16::MAX as i32) as i16;
    node.content_height = height.clamp(0, i16::MAX as i32) as i16;
}
