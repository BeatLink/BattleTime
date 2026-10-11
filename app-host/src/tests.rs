extern crate std;

use std::{
    alloc::{GlobalAlloc, Layout, System as StandardAllocator},
    cell::Cell,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use embedded_graphics::{
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::*,
};
use neotime_runtime::{
    events::{ButtonPress, SwipeDirection},
    instruction::Instruction,
    widgets::Property,
    writer::{CodeWriter, ImageWriter},
    AppKind, ColorValue, EffectKind, Failure, PropertySource, SystemValue, SystemValues, Theme, ThemeColor,
    Value, ValueType, WidgetKind, DEFAULT_THEME,
};

use super::*;
use crate::demo::{stopwatch_image, stopwatch_variables, stopwatch_widgets};

// ============================================================================
// Counting the heap
// ============================================================================

std::thread_local! {
    static LIVE_BYTES: Cell<isize> = const { Cell::new(0) };
    static MOST_LIVE_BYTES: Cell<isize> = const { Cell::new(0) };
}

/// The standard allocator, counting what each thread has allocated and not yet freed.
struct CountingAllocator;

fn count_allocation(change: isize) {
    let _ = LIVE_BYTES.try_with(|live| {
        live.set(live.get() + change);
        let _ = MOST_LIVE_BYTES.try_with(|most| most.set(most.get().max(live.get())));
    });
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation(layout.size() as isize);
        unsafe { StandardAllocator.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        count_allocation(-(layout.size() as isize));
        unsafe { StandardAllocator.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn live_bytes() -> isize {
    LIVE_BYTES.with(Cell::get)
}

fn start_counting_peak() {
    MOST_LIVE_BYTES.with(|most| most.set(live_bytes()));
}

fn most_live_bytes() -> isize {
    MOST_LIVE_BYTES.with(Cell::get)
}

// ============================================================================
// Fake screen
// ============================================================================

const SCREEN_SIDE: usize = 240;

/// A 240x240 screen in memory that remembers the area drawn on since it was last cleared.
struct FakeScreen {
    pixels: Vec<Rgb565>,
    drawn_area: Option<Rect>,
}

impl FakeScreen {
    fn new() -> Self {
        FakeScreen {
            pixels: vec![Rgb565::BLACK; SCREEN_SIDE * SCREEN_SIDE],
            drawn_area: None,
        }
    }

    fn pixel(&self, x: i32, y: i32) -> Rgb565 {
        self.pixels[y as usize * SCREEN_SIDE + x as usize]
    }

    fn count_in(&self, area: Rect, color: Rgb565) -> usize {
        let mut count = 0;
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                count += (self.pixel(x, y) == color) as usize;
            }
        }
        count
    }

    fn forget_drawn_area(&mut self) {
        self.drawn_area = None;
    }
}

impl OriginDimensions for FakeScreen {
    fn size(&self) -> Size {
        Size::new(SCREEN_SIDE as u32, SCREEN_SIDE as u32)
    }
}

impl DrawTarget for FakeScreen {
    type Color = Rgb565;
    type Error = ();

    fn draw_iter<Pixels>(&mut self, pixels: Pixels) -> Result<(), ()>
    where
        Pixels: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for Pixel(point, color) in pixels {
            if point.x < 0 || point.y < 0 || point.x >= SCREEN_SIDE as i32 || point.y >= SCREEN_SIDE as i32 {
                continue;
            }
            self.pixels[point.y as usize * SCREEN_SIDE + point.x as usize] = color;
            let dot = Rect {
                x: point.x,
                y: point.y,
                width: 1,
                height: 1,
            };
            self.drawn_area = Some(self.drawn_area.map_or(dot, |area| area.union(dot)));
        }
        Ok(())
    }
}

fn rgb565(value: u16) -> Rgb565 {
    Rgb565::from(RawU16::new(value))
}

fn contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

fn center_of(bounds: Rect) -> (i32, i32) {
    (bounds.x + bounds.width / 2, bounds.y + bounds.height / 2)
}

// ============================================================================
// Fake system
// ============================================================================

/// A system that records what the app asks of it.
#[derive(Default)]
struct FakeSystem {
    effects: Vec<(EffectKind, Value, Option<String>)>,
    failures: Vec<AppFailure>,
    weather_location: String,
}

impl SystemValues for FakeSystem {
    fn read(&self, value: SystemValue) -> Value {
        match value {
            SystemValue::Uses24HourClock => 1,
            _ => 0,
        }
    }

    fn read_text(&self, value: SystemValue) -> &str {
        match value {
            SystemValue::WeatherLocation => &self.weather_location,
            _ => "",
        }
    }

    fn random_number(&self) -> u32 {
        4
    }
}

impl System for FakeSystem {
    fn carry_out(&mut self, effect: &AppEffect) {
        let text = match effect.kind {
            EffectKind::Go | EffectKind::OpenApp | EffectKind::ShowToast | EffectKind::Log => {
                effect.text(0).map(ToString::to_string)
            }
            _ => None,
        };
        self.effects.push((effect.kind, effect.number(0), text));
    }

    fn app_failed(&mut self, failure: AppFailure) {
        self.failures.push(failure);
    }
}

// ============================================================================
// A counter app
// ============================================================================

/// The widget numbers of the counter app's parts.
mod counter_widgets {
    pub const COUNT_LABEL: u16 = 1;
    pub const FIXED_LABEL: u16 = 2;
    pub const ADD_BUTTON: u16 = 3;
    pub const BROKEN_BUTTON: u16 = 4;
    pub const SETTINGS_BUTTON: u16 = 5;
    pub const SETTINGS_SCREEN: u16 = 6;
    pub const SETTINGS_TITLE: u16 = 7;
    pub const BACK_BUTTON: u16 = 8;
}

const COUNT: u8 = 0;
const BUTTON_HEIGHT: i32 = 40;

fn code(build: impl FnOnce(&mut CodeWriter)) -> Vec<u8> {
    let mut writer = CodeWriter::new();
    build(&mut writer);
    writer.finish().unwrap()
}

/// Two screens: a count with buttons to add one, fail and open settings, and a settings screen.
fn counter_image() -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new("counter", "Counter", AppKind::App, 1);
    writer.variable("count", ValueType::Int, true, 0);
    let main = writer.string("main");
    let settings = writer.string("settings");
    let fixed = writer.string("Fixed");
    let add = writer.string("Add");
    let broken = writer.string("Break");
    let open_settings = writer.string("Settings");
    let back = writer.string("Back");

    let count_text = writer.expression(code(|code| {
        code.emit(ReadState(COUNT)).emit(FormatNumber(1));
    }));
    let add_one = writer.handler(code(|code| {
        code.emit(ReadState(COUNT))
            .emit(PushSmall(1))
            .emit(Add)
            .emit(WriteState(COUNT))
            .emit(PushSmall(20))
            .emit(Effect(EffectKind::Vibrate));
    }));
    let add_then_fail = writer.handler(code(|code| {
        code.emit(ReadState(COUNT))
            .emit(PushSmall(1))
            .emit(Add)
            .emit(WriteState(COUNT))
            .emit(PushSmall(20))
            .emit(Effect(EffectKind::Vibrate))
            .emit(PushSmall(1))
            .emit(PushSmall(0))
            .emit(Divide)
            .emit(Drop);
    }));
    let go_to_settings = writer.handler(code(|code| {
        code.emit(PushText(settings)).emit(Effect(EffectKind::Go));
    }));
    let go_back = writer.handler(code(|code| {
        code.emit(Effect(EffectKind::Back));
    }));

    let constant = PropertySource::Constant;
    let button = |writer: &mut ImageWriter, text: u16, handler: u16| {
        writer.widget(
            WidgetKind::Button,
            0,
            &[
                (Property::Text, constant(text as i32)),
                (Property::Height, constant(BUTTON_HEIGHT)),
                (Property::OnClick, PropertySource::Handler(handler)),
            ],
        );
    };

    writer.widget(
        WidgetKind::Column,
        5,
        &[
            (Property::ScreenName, constant(main as i32)),
            (Property::OnButton, PropertySource::Handler(go_back)),
        ],
    );
    writer.widget(
        WidgetKind::Label,
        0,
        &[(Property::Text, PropertySource::Expression(count_text))],
    );
    writer.widget(WidgetKind::Label, 0, &[(Property::Text, constant(fixed as i32))]);
    button(&mut writer, add, add_one);
    button(&mut writer, broken, add_then_fail);
    button(&mut writer, open_settings, go_to_settings);

    writer.widget(
        WidgetKind::Column,
        2,
        &[(Property::ScreenName, constant(settings as i32))],
    );
    writer.widget(
        WidgetKind::Label,
        0,
        &[(Property::Text, constant(open_settings as i32))],
    );
    button(&mut writer, back, go_back);

    writer.build()
}

const START_TIME: Value = 1_000_000_000;

fn click_on(host: &mut AppHost, widget: u16, now: Value, system: &mut FakeSystem) -> EventResult {
    let (bounds, shown) = host.widget_bounds(widget).unwrap();
    assert!(shown);
    let (x, y) = center_of(bounds);
    host.touch(Touch::Click { x, y }, now, system)
}

fn dirty(host: &AppHost) -> Vec<u16> {
    host.dirty_widgets().collect()
}

// ============================================================================
// Opening
// ============================================================================

#[test]
fn opening_an_image_draws_its_label() {
    let image = stopwatch_image();
    let mut system = FakeSystem::default();
    let mut screen = FakeScreen::new();

    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.draw(&mut screen).unwrap();

    assert_eq!(host.widget_text(stopwatch_widgets::ELAPSED_LABEL), Some("00:00"));
    let (label, shown) = host.widget_bounds(stopwatch_widgets::ELAPSED_LABEL).unwrap();
    assert!(shown);
    let text_color = rgb565(DEFAULT_THEME.color(ThemeColor::Text));
    assert!(screen.count_in(label, text_color) > 100);
    assert!(!host.needs_drawing());
    assert!(system.failures.is_empty());
}

#[test]
fn the_screen_is_laid_out_inside_the_display() {
    let image = stopwatch_image();
    let mut system = FakeSystem::default();

    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    let (label, _) = host.widget_bounds(stopwatch_widgets::ELAPSED_LABEL).unwrap();
    let (start, _) = host.widget_bounds(stopwatch_widgets::START_BUTTON).unwrap();
    let (reset, reset_shown) = host.widget_bounds(stopwatch_widgets::RESET_BUTTON).unwrap();
    assert!(reset_shown);
    for bounds in [label, start, reset] {
        assert!(contains(SCREEN_AREA, bounds), "{bounds:?} is off the screen");
    }
    assert!(label.y + label.height <= start.y);
    assert!(start.y + start.height <= reset.y);
    assert_eq!(start.height, 52);
    assert_eq!(reset.y + reset.height + 4 + 12, 240);
}

#[test]
fn opening_a_broken_image_fails() {
    let mut image = stopwatch_image();
    let last = image.len() - 1;
    image[last] ^= 0xFF;

    let result = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut FakeSystem::default());

    assert!(matches!(result, Err(OpenError::Image(_))));
}

// ============================================================================
// Touch
// ============================================================================

#[test]
fn a_click_runs_the_handler_and_redraws_only_the_affected_widgets() {
    let image = counter_image();
    let mut system = FakeSystem::default();
    let mut screen = FakeScreen::new();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.draw(&mut screen).unwrap();
    screen.forget_drawn_area();

    let result = click_on(&mut host, counter_widgets::ADD_BUTTON, START_TIME, &mut system);

    assert_eq!(result, EventResult::Handled);
    assert_eq!(host.state().get(COUNT), Some(1));
    assert_eq!(system.effects, vec![(EffectKind::Vibrate, 20, None)]);
    assert_eq!(dirty(&host), vec![counter_widgets::COUNT_LABEL]);
    assert_eq!(host.widget_text(counter_widgets::COUNT_LABEL), Some("1"));

    host.draw(&mut screen).unwrap();
    let (label, _) = host.widget_bounds(counter_widgets::COUNT_LABEL).unwrap();
    let drawn = screen.drawn_area.unwrap();
    assert!(
        contains(label, drawn),
        "drew {drawn:?} outside the label {label:?}"
    );
    assert_eq!(host.take_changed_persistent_variables(), 1 << COUNT);
    assert_eq!(host.take_changed_persistent_variables(), 0);
}

#[test]
fn a_touch_with_no_handler_is_not_handled() {
    let image = counter_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let (fixed, _) = host.widget_bounds(counter_widgets::FIXED_LABEL).unwrap();
    let (x, y) = center_of(fixed);

    let click = host.touch(Touch::Click { x, y }, START_TIME, &mut system);
    let swipe = host.touch(Touch::Swipe(SwipeDirection::Right), START_TIME, &mut system);

    assert_eq!([click, swipe], [EventResult::NotHandled, EventResult::NotHandled]);
    assert!(system.effects.is_empty());
}

#[test]
fn a_failing_handler_leaves_the_state_unchanged() {
    let image = counter_image();
    let mut system = FakeSystem::default();
    let mut screen = FakeScreen::new();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.draw(&mut screen).unwrap();

    let result = click_on(&mut host, counter_widgets::BROKEN_BUTTON, START_TIME, &mut system);

    assert_eq!(result, EventResult::Failed);
    assert_eq!(host.state().get(COUNT), Some(0));
    assert!(system.effects.is_empty());
    assert!(matches!(
        system.failures.as_slice(),
        [AppFailure::Handler {
            failure: Failure::DivideByZero,
            ..
        }]
    ));
    assert!(!host.needs_drawing());
    assert_eq!(host.take_changed_persistent_variables(), 0);

    let still_open = click_on(&mut host, counter_widgets::ADD_BUTTON, START_TIME, &mut system);
    assert_eq!(still_open, EventResult::Handled);
    assert_eq!(host.widget_text(counter_widgets::COUNT_LABEL), Some("1"));
}

// ============================================================================
// Screens
// ============================================================================

#[test]
fn go_switches_screens_and_back_returns() {
    let image = counter_image();
    let mut system = FakeSystem::default();
    let mut screen = FakeScreen::new();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.draw(&mut screen).unwrap();

    click_on(
        &mut host,
        counter_widgets::SETTINGS_BUTTON,
        START_TIME,
        &mut system,
    );

    assert_eq!(host.screen(), counter_widgets::SETTINGS_SCREEN);
    assert_eq!(host.screen_name(), Some("settings"));
    assert_eq!(
        host.widget_text(counter_widgets::SETTINGS_TITLE),
        Some("Settings")
    );
    assert_eq!(host.widget_bounds(counter_widgets::ADD_BUTTON), None);
    assert!(host.needs_drawing());
    assert!(system.effects.is_empty());

    click_on(&mut host, counter_widgets::BACK_BUTTON, START_TIME, &mut system);
    assert_eq!(host.screen_name(), Some("main"));
    assert!(system.effects.is_empty());

    host.touch(Touch::Swipe(SwipeDirection::Up), START_TIME, &mut system);
    click_on(
        &mut host,
        counter_widgets::SETTINGS_BUTTON,
        START_TIME,
        &mut system,
    );
    click_on(&mut host, counter_widgets::BACK_BUTTON, START_TIME, &mut system);
    host.draw(&mut screen).unwrap();
    let (add, _) = host.widget_bounds(counter_widgets::ADD_BUTTON).unwrap();
    assert!(screen.count_in(add, rgb565(DEFAULT_THEME.color(ThemeColor::Surface))) > 100);
}

#[test]
fn back_from_the_first_screen_goes_to_the_system() {
    let image = counter_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    click_on(
        &mut host,
        counter_widgets::SETTINGS_BUTTON,
        START_TIME,
        &mut system,
    );
    click_on(&mut host, counter_widgets::BACK_BUTTON, START_TIME, &mut system);

    let result = host.press_button(ButtonPress::Press, START_TIME, &mut system);

    assert_eq!(result, EventResult::Handled);
    assert_eq!(host.screen_name(), Some("main"));
    assert_eq!(system.effects, vec![(EffectKind::Back, 0, None)]);
}

// ============================================================================
// Time
// ============================================================================

#[test]
fn a_tick_redraws_a_clock_bound_label_once_a_second() {
    let image = stopwatch_image();
    let mut system = FakeSystem::default();
    let mut screen = FakeScreen::new();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.draw(&mut screen).unwrap();
    click_on(
        &mut host,
        stopwatch_widgets::START_BUTTON,
        START_TIME,
        &mut system,
    );
    host.draw(&mut screen).unwrap();
    assert_eq!(
        host.state().get(stopwatch_variables::STARTED_AT),
        Some(START_TIME)
    );

    host.tick(START_TIME + 400, &mut system);
    assert!(!host.needs_drawing());
    assert_eq!(host.next_wakeup(START_TIME + 400), Some(START_TIME + 1000));

    host.tick(START_TIME + 1000, &mut system);
    assert_eq!(dirty(&host), vec![stopwatch_widgets::ELAPSED_LABEL]);
    assert_eq!(host.widget_text(stopwatch_widgets::ELAPSED_LABEL), Some("00:01"));
    host.draw(&mut screen).unwrap();

    host.tick(START_TIME + 1999, &mut system);
    assert!(!host.needs_drawing());
    host.tick(START_TIME + 62_000, &mut system);
    assert_eq!(host.widget_text(stopwatch_widgets::ELAPSED_LABEL), Some("01:02"));
    assert_eq!(host.next_wakeup(START_TIME + 62_250), Some(START_TIME + 63_000));
}

#[test]
fn a_screen_without_the_clock_never_wakes() {
    let image = counter_image();
    let mut system = FakeSystem::default();

    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    assert_eq!(host.next_wakeup(START_TIME), None);
}

#[test]
fn stopping_and_resetting_the_stopwatch() {
    let image = stopwatch_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    click_on(
        &mut host,
        stopwatch_widgets::START_BUTTON,
        START_TIME,
        &mut system,
    );
    assert!(!host.widget_bounds(stopwatch_widgets::RESET_BUTTON).unwrap().1);
    assert_eq!(host.widget_text(stopwatch_widgets::START_BUTTON), Some("Stop"));
    click_on(
        &mut host,
        stopwatch_widgets::START_BUTTON,
        START_TIME + 5_500,
        &mut system,
    );
    host.tick(START_TIME + 9_000, &mut system);

    assert_eq!(host.widget_text(stopwatch_widgets::ELAPSED_LABEL), Some("00:05"));
    assert!(host.widget_bounds(stopwatch_widgets::RESET_BUTTON).unwrap().1);
    click_on(
        &mut host,
        stopwatch_widgets::RESET_BUTTON,
        START_TIME + 9_000,
        &mut system,
    );
    assert_eq!(host.widget_text(stopwatch_widgets::ELAPSED_LABEL), Some("00:00"));
    assert_eq!(
        host.take_changed_persistent_variables(),
        1 << stopwatch_variables::STARTED_AT | 1 << stopwatch_variables::STOPPED_AFTER
    );
}

// ============================================================================
// Theme
// ============================================================================

#[test]
fn set_theme_changes_colours_without_running_handlers() {
    let image = stopwatch_image();
    let mut system = FakeSystem::default();
    let mut screen = FakeScreen::new();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.draw(&mut screen).unwrap();
    let (start, _) = host.widget_bounds(stopwatch_widgets::START_BUTTON).unwrap();
    let old_accent = rgb565(DEFAULT_THEME.color(ThemeColor::Accent));
    assert!(screen.count_in(start, old_accent) > 1000);

    let mut light = DEFAULT_THEME;
    light.set_color(ThemeColor::Background, 0xFFFF);
    light.set_color(ThemeColor::Accent, 0xF800);
    host.set_theme(light);
    host.draw(&mut screen).unwrap();

    assert!(system.effects.is_empty());
    assert_eq!(
        host.state().get(stopwatch_variables::STARTED_AT),
        Some(neotime_runtime::NONE)
    );
    assert_eq!(screen.count_in(start, old_accent), 0);
    assert!(screen.count_in(start, rgb565(0xF800)) > 1000);
    assert_eq!(screen.pixel(0, 0), rgb565(0xFFFF));
}

// ============================================================================
// Measurements
// ============================================================================

#[test]
fn the_demo_screen_fits_the_ui_heap() {
    const UI_HEAP_BYTES: isize = 14 * 1024;
    let image = stopwatch_image();
    let mut system = FakeSystem::default();
    let mut screen = FakeScreen::new();
    let before = live_bytes();
    start_counting_peak();

    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let after_open = live_bytes() - before;
    host.draw(&mut screen).unwrap();
    let peak_while_drawing = most_live_bytes() - before;
    let after_draw = live_bytes() - before;

    std::println!(
        "demo screen on the host: {after_open} B held after opening, {after_draw} B after drawing, \
         {peak_while_drawing} B at the peak; AppHost is {} B",
        core::mem::size_of::<AppHost>()
    );
    assert_eq!(after_open, after_draw);
    assert!(peak_while_drawing < UI_HEAP_BYTES);
}

#[test]
fn every_property_value_kind_is_applied() {
    let theme_text = ColorValue::Theme(ThemeColor::Text).to_value();
    let mut theme = Theme::new([0; 8]);
    theme.set_color(ThemeColor::Text, 0x1234);

    assert_eq!(
        ColorValue::from_value(theme_text).unwrap().resolve(&theme),
        0x1234
    );
    assert_eq!(Length::from_value(-50).of(200), 100);
    assert_eq!(Length::from_value(30).of(200), 30);
    assert_eq!(Align::from_value(2), Some(Align::End));
    assert!(is_mapped(WidgetKind::Arc));
    assert!(is_mapped(WidgetKind::Chart));
    assert!(!is_mapped(WidgetKind::TextArea));
}

// ============================================================================
// Effects with text, system values and angles
// ============================================================================

/// One screen: a label showing the count, a label showing the weather's place, a full ring, a
/// default arc, and a button that adds one and toasts the new count.
fn toast_image() -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new("toast", "Toast", AppKind::App, 1);
    writer.variable("count", ValueType::Int, false, 0);
    let add = writer.string("Add");
    let count_text = writer.expression(code(|code| {
        code.emit(ReadState(COUNT)).emit(FormatNumber(3));
    }));
    let location = writer.expression(code(|code| {
        code.emit(ReadSystem(SystemValue::WeatherLocation));
    }));
    let add_and_toast = writer.handler(code(|code| {
        code.emit(ReadState(COUNT))
            .emit(PushSmall(1))
            .emit(Add)
            .emit(Duplicate)
            .emit(WriteState(COUNT))
            .emit(FormatNumber(2))
            .emit(Effect(EffectKind::ShowToast));
    }));

    let constant = PropertySource::Constant;
    writer.widget(WidgetKind::Column, 5, &[]);
    writer.widget(
        WidgetKind::Label,
        0,
        &[(Property::Text, PropertySource::Expression(count_text))],
    );
    writer.widget(
        WidgetKind::Label,
        0,
        &[(Property::Text, PropertySource::Expression(location))],
    );
    for (start, end) in [(Some(0), Some(360)), (None, None)] {
        let mut properties = vec![
            (Property::Width, constant(60)),
            (Property::Height, constant(60)),
            (Property::Align, constant(1)),
        ];
        if let (Some(start), Some(end)) = (start, end) {
            properties.push((Property::StartAngle, constant(start)));
            properties.push((Property::EndAngle, constant(end)));
        }
        writer.widget(WidgetKind::Arc, 0, &properties);
    }
    writer.widget(
        WidgetKind::Button,
        0,
        &[
            (Property::Text, constant(add as i32)),
            (Property::OnClick, PropertySource::Handler(add_and_toast)),
        ],
    );
    writer.build()
}

mod toast_widgets {
    pub const COUNT_LABEL: u16 = 1;
    pub const LOCATION_LABEL: u16 = 2;
    pub const FULL_RING: u16 = 3;
    pub const DEFAULT_ARC: u16 = 4;
    pub const ADD_BUTTON: u16 = 5;
}

#[test]
fn an_effect_reads_text_the_handler_wrote_before_anything_else_runs() {
    let image = toast_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    click_on(&mut host, toast_widgets::ADD_BUTTON, START_TIME, &mut system);
    click_on(&mut host, toast_widgets::ADD_BUTTON, START_TIME, &mut system);

    let toasts: Vec<Option<String>> = system.effects.iter().map(|effect| effect.2.clone()).collect();
    assert_eq!(toasts, vec![Some("01".to_string()), Some("02".to_string())]);
    assert_eq!(host.widget_text(toast_widgets::COUNT_LABEL), Some("002"));
}

#[test]
fn a_grouped_system_value_bit_redraws_what_reads_any_of_the_group() {
    let image = toast_image();
    let mut system = FakeSystem {
        weather_location: "Harbour".to_string(),
        ..FakeSystem::default()
    };
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    assert_eq!(host.widget_text(toast_widgets::LOCATION_LABEL), Some("Harbour"));
    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();

    system.weather_location = "Hills".to_string();
    host.system_values_changed(SystemValue::BatteryPercent.bit(), START_TIME, &mut system);
    assert!(!host.needs_drawing());
    host.system_values_changed(SystemValue::WeatherTemperature.bit(), START_TIME, &mut system);

    assert_eq!(host.widget_text(toast_widgets::LOCATION_LABEL), Some("Hills"));
    assert_eq!(dirty(&host), vec![toast_widgets::LOCATION_LABEL]);
}

#[test]
fn arc_angles_count_clockwise_from_twelve() {
    let image = toast_image();
    let mut system = FakeSystem::default();
    let mut screen = FakeScreen::new();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.draw(&mut screen).unwrap();
    let track = rgb565(DEFAULT_THEME.color(ThemeColor::Surface));
    let ring_at = |bounds: Rect, x: i32, y: i32| Rect {
        x: bounds.x + x - 3,
        y: bounds.y + y - 3,
        width: 6,
        height: 6,
    };

    let (full, _) = host.widget_bounds(toast_widgets::FULL_RING).unwrap();
    let (default, _) = host.widget_bounds(toast_widgets::DEFAULT_ARC).unwrap();

    for (x, y) in [(30, 4), (30, 56), (4, 30), (56, 30)] {
        assert!(
            screen.count_in(ring_at(full, x, y), track) > 0,
            "no ring at {x},{y}"
        );
    }
    assert!(screen.count_in(ring_at(default, 30, 4), track) > 0);
    assert_eq!(screen.count_in(ring_at(default, 30, 56), track), 0);
}

// ============================================================================
// Subscriptions
// ============================================================================

/// Variable 0 counts ticks of a one-second timer, variable 1 counts opens and variable 2 holds steps.
fn subscriber_image(only_while_open: bool) -> Vec<u8> {
    use neotime_runtime::subscriptions::{Subscription, SubscriptionKind, SubscriptionValue};
    use Instruction::*;

    let mut writer = ImageWriter::new("subscriber", "Subscriber", AppKind::App, 1);
    writer.variable("ticks", ValueType::Int, true, 0);
    writer.variable("opens", ValueType::Int, false, 0);
    writer.variable("steps", ValueType::Int, false, 0);
    let add_one_to = |writer: &mut ImageWriter, variable: u8| {
        writer.handler(code(|code| {
            code.emit(ReadState(variable))
                .emit(PushSmall(1))
                .emit(Add)
                .emit(WriteState(variable));
        }))
    };
    let count_tick = add_one_to(&mut writer, 0);
    let count_open = add_one_to(&mut writer, 1);
    let store_steps = writer.handler(code(|code| {
        code.emit(ReadEvent(0)).emit(WriteState(2));
    }));
    let subscription = |kind, handler, value| Subscription {
        kind,
        only_while_open,
        handler,
        detail: 0,
        days: 0,
        value: SubscriptionValue::Constant(value),
    };
    writer.subscription(subscription(SubscriptionKind::Every, count_tick, 60_000));
    writer.subscription(subscription(SubscriptionKind::Open, count_open, 0));
    writer.subscription(subscription(SubscriptionKind::Steps, store_steps, 0));
    let ticks_text = writer.expression(code(|code| {
        code.emit(ReadState(0)).emit(FormatNumber(1));
    }));
    writer.widget(WidgetKind::Column, 1, &[]);
    writer.widget(
        WidgetKind::Label,
        0,
        &[(Property::Text, PropertySource::Expression(ticks_text))],
    );
    writer.build()
}

#[test]
fn an_every_subscription_fires_on_time_and_sets_the_next_wakeup() {
    let image = subscriber_image(false);
    let mut system = FakeSystem::default();
    // Timers line up with the clock, so this starts on a whole minute.
    const START_TIME: Value = 1_000_020_000;
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    assert_eq!(host.next_wakeup(START_TIME), Some(START_TIME + 60_000));

    host.tick(START_TIME + 59_999, &mut system);
    assert_eq!(host.state().get(0).unwrap(), 0);
    host.tick(START_TIME + 60_000, &mut system);
    assert_eq!(host.state().get(0).unwrap(), 1);
    assert_eq!(host.next_wakeup(START_TIME + 60_000), Some(START_TIME + 120_000));
    assert!(host.needs_drawing());
}

#[test]
fn open_subscriptions_run_each_time_the_app_comes_to_the_front() {
    let image = subscriber_image(false);
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    assert_eq!(host.state().get(1).unwrap(), 1);
    host.close(START_TIME + 1, &mut system);
    host.bring_to_front(START_TIME + 2, &mut system);
    assert_eq!(host.state().get(1).unwrap(), 2);
}

#[test]
fn a_closed_app_keeps_background_timers_but_drops_open_only_ones() {
    let mut system = FakeSystem::default();

    let background = subscriber_image(false);
    let mut host = AppHost::open(&background, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.close(START_TIME + 1, &mut system);
    host.tick(START_TIME + 60_000, &mut system);
    assert_eq!(host.state().get(0).unwrap(), 1);

    let open_only = subscriber_image(true);
    let mut host = AppHost::open(&open_only, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.close(START_TIME + 1, &mut system);
    assert_eq!(host.next_wakeup(START_TIME + 1), None);
    host.tick(START_TIME + 60_000, &mut system);
    assert_eq!(host.state().get(0).unwrap(), 0);
}

#[test]
fn a_pushed_event_hands_its_fields_to_the_handler() {
    let image = subscriber_image(false);
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.deliver(
        PushedEvent::Steps { steps_today: 4321 },
        START_TIME + 1,
        &mut system,
    );
    assert_eq!(host.state().get(2).unwrap(), 4321);
}

// ============================================================================
// Saved state
// ============================================================================

#[test]
fn saved_state_comes_back_when_the_app_is_opened_again() {
    const START_TIME: Value = 1_000_020_000;
    let image = subscriber_image(false);
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.tick(START_TIME + 60_000, &mut system);
    host.tick(START_TIME + 120_000, &mut system);

    let mut saved = vec![0; LARGEST_SAVED_STATE_BYTES];
    let length = host.save_changed_state(&mut saved).unwrap().expect("ticks changed");
    assert_eq!(host.save_changed_state(&mut saved).unwrap(), None);

    let host = AppHost::open_with_saved_state(&image, DEFAULT_THEME, &saved[..length], START_TIME, &mut system)
        .unwrap();
    assert_eq!(host.state().get(0).unwrap(), 2);
    // Opens are not persistent, so the new host counts only its own.
    assert_eq!(host.state().get(1).unwrap(), 1);
}

#[test]
fn saved_state_from_another_app_is_ignored() {
    let image = subscriber_image(false);
    let mut system = FakeSystem::default();
    let host =
        AppHost::open_with_saved_state(&image, DEFAULT_THEME, b"not saved state", START_TIME, &mut system).unwrap();
    assert_eq!(host.state().get(0).unwrap(), 0);
}

// ============================================================================
// Images and text
// ============================================================================

const MASK_SIDE: u16 = 8;

/// One 8x8 fully covered mask image drawn in red, and a long label under it.
fn image_and_text_image() -> Vec<u8> {
    use neotime_runtime::resources::ImageFormat;

    let mut writer = ImageWriter::new("pictures", "Pictures", AppKind::App, 1);
    let long_text = writer.string("one two three four five six seven eight nine ten eleven twelve");
    let square = writer.image_resource(
        ImageFormat::Mask,
        MASK_SIDE,
        MASK_SIDE,
        &[255; (MASK_SIDE * MASK_SIDE) as usize],
    );
    let constant = PropertySource::Constant;
    writer.widget(WidgetKind::Column, 2, &[]);
    writer.widget(
        WidgetKind::Image,
        0,
        &[
            (Property::Image, constant(square as i32)),
            (Property::Color, constant(0xF800)),
        ],
    );
    writer.widget(WidgetKind::Label, 0, &[(Property::Text, constant(long_text as i32))]);
    writer.build()
}

#[test]
fn a_mask_image_is_drawn_in_its_colour() {
    let image = image_and_text_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();
    let (bounds, shown) = host.widget_bounds(1).unwrap();
    assert!(shown);
    let red = screen.count_in(bounds, Rgb565::RED);
    assert_eq!(red, (MASK_SIDE * MASK_SIDE) as usize);
}

#[test]
fn a_long_label_wraps_onto_several_lines_inside_the_screen() {
    let image = image_and_text_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();
    let (bounds, _) = host.widget_bounds(2).unwrap();
    assert!(contains(SCREEN_AREA, bounds));
    assert!(bounds.height > 40, "one line would be about 20 pixels, got {bounds:?}");
}

// ============================================================================
// The launcher
// ============================================================================

#[test]
fn the_launcher_opens_an_app_by_its_id() {
    let counter = counter_image();
    let toast = toast_image();
    let launcher = crate::launcher::launcher_image(&[&counter, b"not an image", &toast]);
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&launcher, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    assert_eq!(host.node_count(), 3);

    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();
    assert_eq!(host.widget_text(2), Some("Toast"));
    click_on(&mut host, 2, START_TIME, &mut system);
    let (kind, _, id) = system.effects.last().unwrap();
    assert_eq!((*kind, id.as_deref()), (EffectKind::OpenApp, Some("toast")));
}
