//! Draws one node: builds its rlvgl widget from the node's values, or draws widgets the host
//! shapes itself, such as cell grids, dials and lists of options, straight from the app's state.
//!
//! Array-backed widgets read their arrays at draw time rather than copying them, so a 400-cell
//! board costs no more UI heap than a label.

use neotime_runtime::{
    resources::{library_image_named_by, ImageFormat, ImageReference, ImageResource},
    AppImage, AppState, ColorValue, Theme, ThemeColor, Value,
};
use rlvgl_core::{
    draw::{draw_widget_bg, fill_rounded_rect},
    raster::PointF,
    renderer::Renderer,
    style::Style,
    widget::{Color, Rect, Widget},
};

use crate::fonts::{line_height, text_width, FontName};
use crate::layout::{TICK_BOX_GAP, TICK_BOX_SIZE};
use crate::nodes::{
    inset, resolve, resolve_or_clear, to_color, Align, Extra, LongMode, Node, Settings, OPAQUE, TRANSPARENT,
};
use crate::text::{draw_line, WrappedLines};

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
}

impl<'image> DrawContext<'_, 'image> {
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

    /// The image an `image` or `icon` value names, from the app or the shared library.
    pub fn image_resource(&self, value: i32) -> Option<ImageResource<'image>> {
        let bytes = match ImageReference::from_value(value)? {
            ImageReference::App(index) => self.image.resource_bytes(index)?,
            ImageReference::SharedLibrary(index) => return self.library?.resource(index),
            ImageReference::InstalledApp(index) => self.installed_app_icons.get(index as usize)?,
        };
        match library_image_named_by(bytes) {
            Some(library_image) => self.library?.resource(library_image),
            None => ImageResource::parse(bytes),
        }
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

/// App angles count clockwise from twelve o'clock and rlvgl's from three, so arcs turn back 90 degrees.
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
        border_width: to_u8(settings.border_width.map(i32::from).unwrap_or(BORDER_WIDTH)),
        alpha: OPAQUE,
        radius: to_u8(settings.radius.map(i32::from).unwrap_or(default_radius)),
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
// Drawing over a known colour
// ============================================================================

/// Passes drawing on with the soft edges of text and masks already mixed with `backdrop`.
///
/// The screen renderer mixes them with the colour behind the whole widget, which is wrong for parts
/// a widget draws over a colour of its own, such as the rows of an AppList.
pub struct OverColorRenderer<'inner> {
    pub inner: &'inner mut dyn Renderer,
    pub backdrop: Color,
}

impl Renderer for OverColorRenderer<'_> {
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.inner.fill_rect(rect, color);
    }

    fn blend_rect(&mut self, rect: Rect, color: Color) {
        self.inner.blend_rect(rect, color);
    }

    fn blend_row(&mut self, x: i32, y: i32, color: Color, coverage: &[u8]) {
        let mix = |front: u8, back: u8, amount: u8| {
            ((front as u16 * amount as u16 + back as u16 * (OPAQUE - amount) as u16) / OPAQUE as u16) as u8
        };
        for (offset, &amount) in coverage.iter().enumerate() {
            if amount == 0 {
                continue;
            }
            let mixed = Color(
                mix(color.0, self.backdrop.0, amount),
                mix(color.1, self.backdrop.1, amount),
                mix(color.2, self.backdrop.2, amount),
                color.3,
            );
            let dot = Rect {
                x: x + offset as i32,
                y,
                width: 1,
                height: 1,
            };
            self.inner.blend_rect(dot, mixed);
        }
    }

    fn draw_pixels(&mut self, position: (i32, i32), pixels: &[Color], width: u32, height: u32) {
        self.inner.draw_pixels(position, pixels, width, height);
    }

    fn draw_text(&mut self, position: (i32, i32), text: &str, color: Color) {
        self.inner.draw_text(position, text, color);
    }
}

// ============================================================================
// Opacity
// ============================================================================

/// Passes drawing on with every colour made more see-through, for `opacity` and `disabled`.
pub struct FadedRenderer<'inner> {
    pub inner: &'inner mut dyn Renderer,
    pub opacity: u8,
}

impl FadedRenderer<'_> {
    fn fade(&self, color: Color) -> Color {
        let alpha = (color.3 as u16 * self.opacity as u16 / OPAQUE as u16) as u8;
        with_alpha(color, alpha)
    }
}

impl Renderer for FadedRenderer<'_> {
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        let faded = self.fade(color);
        self.inner.blend_rect(rect, faded);
    }

    fn blend_rect(&mut self, rect: Rect, color: Color) {
        let faded = self.fade(color);
        self.inner.blend_rect(rect, faded);
    }

    fn blend_row(&mut self, x: i32, y: i32, color: Color, coverage: &[u8]) {
        let faded = self.fade(color);
        self.inner.blend_row(x, y, faded, coverage);
    }

    fn draw_pixels(&mut self, position: (i32, i32), pixels: &[Color], width: u32, height: u32) {
        let width = width as usize;
        for (row_index, row) in pixels.chunks(width.max(1)).take(height as usize).enumerate() {
            for (column, &pixel) in row.iter().enumerate() {
                let dot = Rect {
                    x: position.0 + column as i32,
                    y: position.1 + row_index as i32,
                    width: 1,
                    height: 1,
                };
                let faded = self.fade(pixel);
                self.inner.blend_rect(dot, faded);
            }
        }
    }

    fn draw_text(&mut self, position: (i32, i32), text: &str, color: Color) {
        let faded = self.fade(color);
        self.inner.draw_text(position, text, faded);
    }
}

// ============================================================================
// Drawing a node
// ============================================================================

/// Draws one node into `renderer`, which the caller has already clipped.
pub fn draw_node(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let settings = &node.settings;
    let theme = context.theme;
    let bounds = node.bounds;
    let accent = |color: Option<ColorValue>| resolve(color, ThemeColor::Accent, theme);
    let track = |color: Option<ColorValue>| resolve(color, ThemeColor::Surface, theme);
    let plain_frame = |renderer: &mut dyn Renderer| {
        let style = frame_style(settings, resolve_or_clear(settings.background, theme), 0, theme);
        draw_widget_bg(renderer, bounds, &style);
    };

    use neotime_runtime::WidgetKind::*;
    match node.kind {
        Label => {
            plain_frame(renderer);
            draw_label_text(node, context, renderer);
        }
        Button => {
            let mut body = rlvgl_widgets::button::Button::new(alloc::string::String::new(), bounds);
            *body.style_mut() = frame_style(settings, track(settings.background), BUTTON_RADIUS, theme);
            body.draw(renderer);
            draw_button_content(node, context, renderer);
        }
        Arc => {
            let mut arc = rlvgl_widgets::arc::Arc::new(bounds, settings.minimum, settings.maximum);
            let (start, end) = match (
                settings.start_angle.map(i32::from),
                settings.end_angle.map(i32::from),
            ) {
                (Some(start), Some(end)) => (start, end),
                _ => (ARC_DEFAULT_START_ANGLE, ARC_DEFAULT_END_ANGLE),
            };
            arc.set_bg_angles(start, end);
            arc.set_rotation(TWELVE_TO_RIGHT_IN_DEGREES);
            arc.set_value(settings.value);
            arc.style.border_color = track(settings.background);
            arc.style.border_width = to_u8(settings.border_width.map(i32::from).unwrap_or(ARC_STROKE_WIDTH));
            arc.indicator_color = accent(settings.color);
            arc.knob_color = TRANSPARENT;
            arc.draw(renderer);
        }
        Bar => {
            let mut bar = rlvgl_widgets::bar::Bar::new(bounds, settings.minimum, settings.maximum);
            bar.set_value(settings.value);
            bar.style = frame_style(settings, track(settings.background), BAR_RADIUS, theme);
            bar.indicator_color = accent(settings.color);
            bar.draw(renderer);
        }
        Led => {
            let mut led = rlvgl_widgets::led::Led::new(bounds);
            led.set_color(accent(settings.color));
            led.set_brightness(to_u8(settings.value));
            led.draw(renderer);
        }
        Switch => {
            let mut switch = rlvgl_widgets::switch::Switch::new(bounds);
            let is_on = settings.value != 0;
            let on_track = if is_on {
                accent(settings.background)
            } else {
                track(settings.background)
            };
            switch.style = frame_style(settings, on_track, bounds.height / 2, theme);
            switch.knob_color = resolve(settings.color, ThemeColor::Text, theme);
            switch.set_on(is_on);
            switch.draw(renderer);
        }
        Slider => {
            let mut slider = rlvgl_widgets::slider::Slider::new(bounds, settings.minimum, settings.maximum);
            slider.style = frame_style(settings, track(settings.background), bounds.height / 2, theme);
            slider.knob_color = accent(settings.color);
            slider.set_value(settings.value);
            slider.draw(renderer);
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
    node.kind == neotime_runtime::WidgetKind::Label
        && node.settings.long_mode == LongMode::Scroll
        && text_width(node.settings.font.metrics(), &node.text)
            > inset(node.bounds, i32::from(node.settings.padding)).width
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
    let area = inset(node.bounds, i32::from(settings.padding));
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
    let area = inset(node.bounds, i32::from(settings.padding));
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
        .image_resource(reference)
        .map_or((PLACEHOLDER_SIZE, PLACEHOLDER_SIZE), |image| {
            (image.width as i32, image.height as i32)
        });
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
    let area = inset(node.bounds, i32::from(settings.padding));
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

    if node.kind == neotime_runtime::WidgetKind::Radio {
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
    use neotime_runtime::resources::library_icon;
    let side = icon_size as i32;
    let has_icons = node.kind == neotime_runtime::WidgetKind::StatusIcons && status_flags & 0b111 != 0;
    let mut pieces = [None; 6];
    let mut count = 0;
    let mut add = |piece: StatusPiece| {
        pieces[count] = Some(piece);
        count += 1;
    };
    if node.kind == neotime_runtime::WidgetKind::StatusIcons {
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
    let inner = inset(node.bounds, i32::from(node.settings.padding));
    let percent = node.settings.value.clamp(0, 100);
    let is_status = node.kind == neotime_runtime::WidgetKind::StatusIcons;
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
                    ImageReference::SharedLibrary(neotime_runtime::resources::library_icon::CHARGING);
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
    let inner = inset(node.bounds, i32::from(node.settings.padding));
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

/// How many apps an AppList holds: the host keeps the count in `value` and the names in its text.
fn app_count(node: &Node) -> i32 {
    node.settings.value.max(0)
}

/// Where each row of an AppList starts: the first row's top and the distance to the next.
fn app_rows(node: &Node) -> (i32, i32) {
    let top = node.bounds.y + i32::from(node.settings.padding) - i32::from(node.scroll_offset);
    (top, APP_ROW_HEIGHT + i32::from(node.settings.gap))
}

/// How tall an AppList's rows are together, with its padding, to bound how far it scrolls.
pub fn app_list_height(node: &Node) -> i32 {
    let count = app_count(node);
    let gaps = (count - 1).max(0) * i32::from(node.settings.gap);
    count * APP_ROW_HEIGHT + gaps + 2 * i32::from(node.settings.padding)
}

/// The place of the app whose row is under a touch, or none between rows and past the last.
pub fn app_row_at(node: &Node, x: i32, y: i32) -> Option<i32> {
    let (top, pitch) = app_rows(node);
    let padding = i32::from(node.settings.padding);
    let inside = x >= node.bounds.x + padding && x < node.bounds.x + node.bounds.width - padding;
    if !inside || y < top {
        return None;
    }
    let place = (y - top) / pitch;
    let in_row = (y - top) % pitch < APP_ROW_HEIGHT;
    (in_row && place < app_count(node)).then_some(place)
}

/// Each row of an AppList that is at least partly inside it: the app's place, its name and the row's box.
pub fn app_list_rows(node: &Node) -> impl Iterator<Item = (usize, &str, Rect)> + '_ {
    let padding = i32::from(node.settings.padding);
    let (top, pitch) = app_rows(node);
    node.text
        .split('\n')
        .take(app_count(node) as usize)
        .enumerate()
        .map(move |(place, name)| {
            let row = Rect {
                x: node.bounds.x + padding,
                y: top + place as i32 * pitch,
                width: (node.bounds.width - 2 * padding).max(0),
                height: APP_ROW_HEIGHT,
            };
            (place, name, row)
        })
        .filter(|(_, _, row)| {
            row.y + row.height > node.bounds.y && row.y < node.bounds.y + node.bounds.height
        })
}

/// Each installed app as a row like a button: its icon, then its name.
fn draw_app_list(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let theme = context.theme;
    let color = resolve(node.settings.color, ThemeColor::Text, theme);
    let surface = resolve(None, ThemeColor::Surface, theme);
    for (place, name, row) in app_list_rows(node) {
        let mut body = rlvgl_widgets::button::Button::new(alloc::string::String::new(), row);
        *body.style_mut() = frame_style(&Settings::default(), surface, BUTTON_RADIUS, theme);
        body.draw(renderer);

        let icon_area = Rect {
            x: row.x + APP_ROW_PADDING,
            y: row.y + (APP_ROW_HEIGHT - APP_ICON_SIZE) / 2,
            width: APP_ICON_SIZE,
            height: APP_ICON_SIZE,
        };
        let icon = ImageReference::InstalledApp(place as u16).to_value();
        let mut over_row = OverColorRenderer {
            inner: renderer,
            backdrop: surface,
        };
        draw_image(context, icon, icon_area, color, &mut over_row);
        let text_left = icon_area.x + APP_ICON_SIZE + APP_ICON_GAP;
        let text_area = Rect {
            x: text_left,
            y: row.y,
            width: (row.x + row.width - APP_ROW_PADDING - text_left).max(0),
            height: APP_ROW_HEIGHT,
        };
        draw_text_block(
            &mut over_row,
            node.settings.font,
            name,
            text_area,
            Align::Start,
            false,
            color,
        );
    }
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
    let area = inset(node.bounds, i32::from(node.settings.padding));
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

fn draw_cell_grid(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let Some(cells) = cell_layout(node) else {
        return;
    };
    let Extra::CellGrid {
        cell_shape, palette, ..
    } = *node.extra
    else {
        return;
    };
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
        if color.3 == 0 {
            continue;
        }
        let place = Rect {
            x: cells.left + (cell % cells.columns) * cells.pitch,
            y: cells.top + (cell / cells.columns) * cells.pitch,
            width: cells.cell_size,
            height: cells.cell_size,
        };
        match cell_shape {
            0 => renderer.fill_rect(place, color),
            1 => fill_rounded_rect(renderer, place, color, to_u8(cells.cell_size / 4)),
            _ => renderer.fill_disc_aa(centre_of(place), cells.cell_size as f32 / 2.0, color),
        }
    }
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

/// The lines a TextFile shows at its scroll offset, as the first and one past the last.
pub fn text_file_lines_in_view(node: &Node) -> (i32, i32) {
    let height = line_height(node.settings.font.metrics()).max(1);
    let top = i32::from(node.scroll_offset) - i32::from(node.settings.padding);
    let first = (top / height).max(0);
    let end = (top + node.bounds.height + height - 1) / height + 1;
    (first, end.max(first))
}

/// Each line a TextFile holds that is at least partly inside it, with its top.
pub fn text_file_rows(node: &Node) -> impl Iterator<Item = (&str, i32)> + '_ {
    let (_, first_line) = text_file_lines(node);
    let height = line_height(node.settings.font.metrics());
    let top = node.bounds.y + i32::from(node.settings.padding) - i32::from(node.scroll_offset);
    node.text
        .split('\n')
        .enumerate()
        .filter(|_| !node.text.is_empty())
        .map(move |(place, line)| (line, top + (first_line + place as i32) * height))
        .filter(move |(_, line_top)| {
            *line_top + height > node.bounds.y && *line_top < node.bounds.y + node.bounds.height
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
    let left = node.bounds.x + i32::from(node.settings.padding);
    let height = line_height(font.metrics());
    let painted = |top: i32| top + height > context.clip.y && top < context.clip.y + context.clip.height;
    for (line, top) in text_file_rows(node).filter(|(_, top)| painted(*top)) {
        draw_line(renderer, font, line, left, top + ascent, color);
    }
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
        .map_or(LINE_WIDTH, |width| width.max(1) as f32);
    let point = |pair: usize| {
        PointF::new(
            node.bounds.x as f32 + context.element(points, 2 * pair) as f32,
            node.bounds.y as f32 + context.element(points, 2 * pair + 1) as f32,
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
    let area = inset(node.bounds, i32::from(settings.padding).max(1));
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
    let start = settings.start_angle.map(i32::from).unwrap_or(0) as f32;
    let end = settings.end_angle.map(i32::from).unwrap_or(360) as f32;
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
    let area = inset(node.bounds, i32::from(settings.padding));
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
    let area = inset(node.bounds, i32::from(node.settings.padding));
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
    let mut coverage = [0u8; LONGEST_SPAN];
    let mut pixel = 0i32;
    for run in image.runs() {
        let mut left = run.count as i32;
        while left > 0 {
            let row = pixel / width;
            let column = pixel % width;
            let span = left.min(width - column);
            let (y_from, y_to) = (scaled_y(row), scaled_y(row + 1));
            let (x_from, x_to) = (scaled_x(column), scaled_x(column + span));
            if y_to > y_from && x_to > x_from {
                let x = place.x + x_from;
                let draw_span = x_to - x_from;
                let mask_coverage = match (image.format, recolors) {
                    (ImageFormat::Mask, _) => Some(run.value as u8),
                    (ImageFormat::Color, true) => Some(brightness(run.value)),
                    (ImageFormat::Color, false) => None,
                };
                for y in place.y + y_from..place.y + y_to {
                    match mask_coverage {
                        None => renderer.fill_rect(
                            Rect {
                                x,
                                y,
                                width: draw_span,
                                height: 1,
                            },
                            to_color(run.value),
                        ),
                        Some(0) => {}
                        Some(OPAQUE) => renderer.fill_rect(
                            Rect {
                                x,
                                y,
                                width: draw_span,
                                height: 1,
                            },
                            color,
                        ),
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
            pixel += span;
            left -= span;
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
    let area = inset(node.bounds, i32::from(node.settings.padding));
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
        let is_highlighted = number == settings.value;
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
    let fit = (node.bounds.height / option_row_height(node).max(1)).max(1);
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
    let top = node.bounds.y + Align::Center.offset(node.bounds.height, rows * row_height);
    let chosen = settings.value;
    let band = Rect {
        x: node.bounds.x,
        y: top + rows / 2 * row_height,
        width: node.bounds.width,
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
            ..node.bounds
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
        height: node.bounds.height + node.option_count as i32 * option_row_height(node),
        ..node.bounds
    }
}

fn draw_dropdown(node: &Node, context: &DrawContext, renderer: &mut dyn Renderer) {
    let settings = &node.settings;
    let theme = context.theme;
    let surface = resolve(settings.background, ThemeColor::Surface, theme);
    let text_color = resolve(settings.color, ThemeColor::Text, theme);
    let style = frame_style(settings, surface, BUTTON_RADIUS / 2, theme);
    draw_widget_bg(renderer, node.bounds, &style);

    let options = context.array(settings.array);
    let text_of = |index: i32| match options {
        Some((variable, length)) if (0..length as i32).contains(&index) => {
            context.text_element(variable, index as usize)
        }
        _ => "",
    };
    let area = inset(node.bounds, OPTION_ROW_PADDING);
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
        y: node.bounds.y + node.bounds.height,
        height: length as i32 * row_height,
        ..node.bounds
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
    let below = y - (node.bounds.y + node.bounds.height);
    (below >= 0).then(|| below / option_row_height(node).max(1))
}

// ============================================================================
// Measuring
// ============================================================================

const MATRIX_ROW_HEIGHT: i32 = 40;

/// Works out the size a node's images or arrays want, for layout.
pub fn measure_node(node: &mut Node, context: &DrawContext) {
    use neotime_runtime::WidgetKind::*;
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
                .image_resource(reference)
                .map_or((PLACEHOLDER_SIZE, PLACEHOLDER_SIZE), |image| {
                    (image.width as i32, image.height as i32)
                }),
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
