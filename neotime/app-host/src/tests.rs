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
    events::SwipeDirection,
    instruction::Instruction,
    widgets::Property,
    writer::{CodeWriter, ImageWriter},
    AppKind, ColorValue, EffectKind, Failure, PropertySource, SystemValue, SystemValues, Theme, ThemeColor,
    Value, ValueType, WidgetKind, DEFAULT_THEME,
};

use super::*;
use crate::demo::{stopwatch_image, stopwatch_variables, stopwatch_widgets};
use crate::fonts::FontName;

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
    steps_today: Value,
    is_phone_connected: bool,
    app_names: Vec<String>,
    /// Each launcher group's name and the place of its first app.
    groups: Vec<(String, Value)>,
    files: Vec<(String, Vec<u8>)>,
}

impl SystemValues for FakeSystem {
    fn read(&self, value: SystemValue) -> Value {
        match value {
            SystemValue::Uses24HourClock => 1,
            SystemValue::StepsToday => self.steps_today,
            SystemValue::IsPhoneConnected => self.is_phone_connected as Value,
            SystemValue::InstalledAppCount => self.app_names.len() as Value,
            SystemValue::LauncherGroupCount => self.groups.len() as Value,
            _ => 0,
        }
    }

    fn read_element(&self, value: SystemValue, index: Value) -> Value {
        match value {
            SystemValue::LauncherGroupFirst => usize::try_from(index)
                .ok()
                .and_then(|index| self.groups.get(index))
                .map_or(neotime_runtime::NONE, |(_, first)| *first),
            _ => neotime_runtime::NONE,
        }
    }

    fn read_element_text(&self, value: SystemValue, index: Value) -> &str {
        match value {
            SystemValue::InstalledAppName => usize::try_from(index)
                .ok()
                .and_then(|index| self.app_names.get(index))
                .map_or("", String::as_str),
            SystemValue::LauncherGroupName => usize::try_from(index)
                .ok()
                .and_then(|index| self.groups.get(index))
                .map_or("", |(name, _)| name.as_str()),
            _ => "",
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

    fn read_file(&self, path: &str, offset: u32, out: &mut [u8]) -> Option<usize> {
        let (_, bytes) = self.files.iter().find(|(name, _)| name == path)?;
        let rest = bytes.get(offset as usize..).unwrap_or(&[]);
        let length = rest.len().min(out.len());
        out[..length].copy_from_slice(&rest[..length]);
        Some(length)
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
        &[(Property::ScreenName, constant(main as i32))],
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
        &[
            (Property::ScreenName, constant(settings as i32)),
            (Property::OnSwipe, PropertySource::Handler(go_back)),
        ],
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

#[test]
fn every_way_of_opening_verifies_the_code() {
    let mut writer = ImageWriter::new("broken", "Broken", AppKind::App, 1);
    writer.expression(vec![]);
    let image = writer.build();
    let refused = |result: Result<(), OpenError>| {
        matches!(
            result,
            Err(OpenError::Image(neotime_runtime::ImageError::BadCode { .. }))
        )
    };
    let mut system = FakeSystem::default();

    assert!(refused(
        AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).map(|_| ())
    ));
    assert!(refused(
        AppHost::open_in_background(&image, DEFAULT_THEME, &[], START_TIME, &mut system).map(|_| ())
    ));
    let mut slot = None;
    assert!(refused(
        AppHost::open_into(&mut slot, &image, DEFAULT_THEME, &[], START_TIME, &mut system).map(|_| ())
    ));
    assert!(slot.is_none());
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
fn a_screen_reached_by_a_swipe_slides_the_way_the_swipe_went() {
    let image = counter_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    click_on(
        &mut host,
        counter_widgets::SETTINGS_BUTTON,
        START_TIME,
        &mut system,
    );
    assert_eq!(host.next_transition(), Transition::Left);

    host.touch(Touch::Swipe(SwipeDirection::Down), START_TIME, &mut system);
    assert_eq!(host.screen_name(), Some("main"));
    assert_eq!(host.next_transition(), Transition::Down);

    click_on(
        &mut host,
        counter_widgets::SETTINGS_BUTTON,
        START_TIME,
        &mut system,
    );
    click_on(&mut host, counter_widgets::BACK_BUTTON, START_TIME, &mut system);
    assert_eq!(host.next_transition(), Transition::Right);
}

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
fn the_button_goes_back_a_screen_and_leaves_the_first_to_the_system() {
    let image = counter_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    click_on(
        &mut host,
        counter_widgets::SETTINGS_BUTTON,
        START_TIME,
        &mut system,
    );

    let from_settings = host.go_back_a_screen(START_TIME, &mut system);
    let from_main = host.go_back_a_screen(START_TIME, &mut system);

    assert_eq!(
        [from_settings, from_main],
        [EventResult::Handled, EventResult::NotHandled]
    );
    assert_eq!(host.screen_name(), Some("main"));
    assert!(system.effects.is_empty());
}

/// Three screens in a row, the last with a button that goes back twice.
fn three_screen_image() -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new("steps", "Steps", AppKind::App, 1);
    let screen_names = [
        writer.string("main"),
        writer.string("middle"),
        writer.string("last"),
    ];
    let label = writer.string("Next");
    let go_to_middle = writer.handler(code(|code| {
        code.emit(PushText(screen_names[1])).emit(Effect(EffectKind::Go));
    }));
    let go_to_last = writer.handler(code(|code| {
        code.emit(PushText(screen_names[2])).emit(Effect(EffectKind::Go));
    }));
    let go_back_twice = writer.handler(code(|code| {
        code.emit(Effect(EffectKind::Back)).emit(Effect(EffectKind::Back));
    }));

    let constant = PropertySource::Constant;
    for (screen_name, handler) in screen_names
        .into_iter()
        .zip([go_to_middle, go_to_last, go_back_twice])
    {
        writer.widget(
            WidgetKind::Column,
            1,
            &[(Property::ScreenName, constant(screen_name as i32))],
        );
        writer.widget(
            WidgetKind::Button,
            0,
            &[
                (Property::Text, constant(label as i32)),
                (Property::Height, constant(BUTTON_HEIGHT)),
                (Property::OnClick, PropertySource::Handler(handler)),
            ],
        );
    }
    writer.build()
}

#[test]
fn a_handler_going_back_twice_retraces_two_screens() {
    let image = three_screen_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    click_on(&mut host, 1, START_TIME, &mut system);
    click_on(&mut host, 3, START_TIME, &mut system);
    assert_eq!(host.screen_name(), Some("last"));

    click_on(&mut host, 5, START_TIME, &mut system);

    assert_eq!(host.screen_name(), Some("main"));
    assert_eq!(
        host.go_back_a_screen(START_TIME, &mut system),
        EventResult::NotHandled
    );
    assert!(system.effects.is_empty());
}

#[test]
fn an_app_can_start_on_a_named_screen_with_nothing_to_go_back_to() {
    let image = counter_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    assert!(!host.start_on_screen("missing", START_TIME, &mut system));
    assert!(host.start_on_screen("settings", START_TIME, &mut system));
    assert_eq!(host.screen_name(), Some("settings"));
    assert_eq!(
        host.go_back_a_screen(START_TIME, &mut system),
        EventResult::NotHandled
    );

    host.close(START_TIME, &mut system);
    assert!(!host.start_on_screen("main", START_TIME, &mut system));
}

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
fn with_the_screen_dark_only_subscriptions_set_the_next_wakeup() {
    let mut system = FakeSystem::default();
    const START_TIME: Value = 1_000_020_000;
    let clock = stopwatch_image();
    let subscriber = subscriber_image(false);
    let mut clock_host = AppHost::open(&clock, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let subscriber_host = AppHost::open(&subscriber, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    clock_host.draw(&mut FakeScreen::new()).unwrap();
    click_on(
        &mut clock_host,
        stopwatch_widgets::START_BUTTON,
        START_TIME,
        &mut system,
    );

    assert!(clock_host.next_wakeup(START_TIME).is_some());
    assert_eq!(clock_host.next_subscription_due(START_TIME), None);
    assert_eq!(
        subscriber_host.next_subscription_due(START_TIME),
        Some(START_TIME + 60_000)
    );
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
// Sensor subscriptions
// ============================================================================

/// An app with an acceleration subscription at this period, if given, and a shake subscription, if asked.
fn sensor_image(acceleration_period: Option<Value>, listens_for_shake: bool) -> Vec<u8> {
    use neotime_runtime::subscriptions::{Subscription, SubscriptionKind, SubscriptionValue};
    use Instruction::*;

    let mut writer = ImageWriter::new("sensors", "Sensors", AppKind::App, 1);
    writer.variable("readings", ValueType::Int, false, 0);
    let count_reading = writer.handler(code(|code| {
        code.emit(ReadState(0))
            .emit(PushSmall(1))
            .emit(Add)
            .emit(WriteState(0));
    }));
    let subscription = |kind, detail, value| Subscription {
        kind,
        only_while_open: true,
        handler: count_reading,
        detail,
        days: 0,
        value: SubscriptionValue::Constant(value),
    };
    if let Some(period) = acceleration_period {
        writer.subscription(subscription(SubscriptionKind::Acceleration, 0, period));
    }
    if listens_for_shake {
        writer.subscription(subscription(SubscriptionKind::Motion, MotionKind::Shake as u8, 0));
    }
    writer.widget(WidgetKind::Column, 0, &[]);
    writer.build()
}

#[test]
fn an_app_listens_only_for_the_events_its_running_subscriptions_wait_for() {
    use neotime_runtime::subscriptions::SubscriptionKind;
    let mut system = FakeSystem::default();
    let image = sensor_image(None, true);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    assert!(host.listens_for(SubscriptionKind::Motion, Some(MotionKind::Shake as u8)));
    assert!(host.listens_for(SubscriptionKind::Motion, None));
    assert!(!host.listens_for(SubscriptionKind::Motion, Some(MotionKind::Tap as u8)));
    assert!(!host.listens_for(SubscriptionKind::Notification, None));

    host.close(START_TIME + 1, &mut system);
    assert!(!host.listens_for(SubscriptionKind::Motion, None));
}

#[test]
fn the_accelerometer_is_read_only_as_often_as_the_open_app_needs() {
    let mut system = FakeSystem::default();
    let reading_period = |image: &[u8], system: &mut FakeSystem| {
        let host = AppHost::open(image, DEFAULT_THEME, START_TIME, system).unwrap();
        motion::reading_period_in_milliseconds(&host)
    };

    assert_eq!(reading_period(&sensor_image(None, false), &mut system), None);
    assert_eq!(
        reading_period(&sensor_image(Some(250), false), &mut system),
        Some(250)
    );
    assert_eq!(
        reading_period(&sensor_image(Some(20), false), &mut system),
        Some(100)
    );
    assert_eq!(reading_period(&sensor_image(None, true), &mut system), Some(100));
    assert_eq!(
        reading_period(&sensor_image(Some(500), true), &mut system),
        Some(100)
    );

    let image = sensor_image(Some(250), true);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.close(START_TIME + 1, &mut system);
    assert_eq!(motion::reading_period_in_milliseconds(&host), None);
}

#[test]
fn an_acceleration_subscription_gets_the_latest_reading_at_its_period() {
    let mut system = FakeSystem::default();
    let image = sensor_image(Some(100), false);
    const START_ON_A_PERIOD: Value = 1_000_000_000;
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_ON_A_PERIOD, &mut system).unwrap();

    let reading = PushedEvent::Acceleration {
        x: 10,
        y: -20,
        z: 980,
    };
    assert_eq!(
        host.deliver(reading, START_ON_A_PERIOD + 10, &mut system),
        EventResult::NotHandled
    );
    assert_eq!(host.state().get(0).unwrap(), 0);
    host.tick(START_ON_A_PERIOD + 100, &mut system);

    assert_eq!(host.state().get(0).unwrap(), 1);
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
    let length = host
        .save_changed_state(&mut saved)
        .unwrap()
        .expect("ticks changed");
    // The size the runtime works out from the image is exactly what is written, so the shell allocates no more.
    assert_eq!(
        length,
        neotime_runtime::memory_need::saved_state_bytes(&neotime_runtime::AppImage::parse(&image).unwrap())
    );
    assert_eq!(host.save_changed_state(&mut saved).unwrap(), None);

    let host =
        AppHost::open_with_saved_state(&image, DEFAULT_THEME, &saved[..length], START_TIME, &mut system)
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
        AppHost::open_with_saved_state(&image, DEFAULT_THEME, b"not saved state", START_TIME, &mut system)
            .unwrap();
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
    writer.widget(
        WidgetKind::Label,
        0,
        &[(Property::Text, constant(long_text as i32))],
    );
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

/// An 8x8 mask covered only in its top-left 2x2 corner, drawn in a box half that size on each side.
fn corner_mask_image() -> Vec<u8> {
    use neotime_runtime::resources::ImageFormat;

    let mut writer = ImageWriter::new("corner", "Corner", AppKind::App, 1);
    let side = MASK_SIDE as usize;
    let mut pixels = vec![0u16; side * side];
    for row in 0..2 {
        for column in 0..2 {
            pixels[row * side + column] = 255;
        }
    }
    let corner = writer.image_resource(ImageFormat::Mask, MASK_SIDE, MASK_SIDE, &pixels);
    let constant = PropertySource::Constant;
    writer.widget(WidgetKind::Column, 1, &[]);
    writer.widget(
        WidgetKind::Image,
        0,
        &[
            (Property::Image, constant(corner as i32)),
            (Property::Color, constant(0xF800)),
            (Property::Width, constant((MASK_SIDE / 2) as i32)),
            (Property::Height, constant((MASK_SIDE / 2) as i32)),
        ],
    );
    writer.build()
}

#[test]
fn an_icon_smaller_than_its_image_still_shows_the_images_corner() {
    let image = corner_mask_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();
    let (bounds, shown) = host.widget_bounds(1).unwrap();
    assert!(shown);
    assert_eq!(bounds.width, (MASK_SIDE / 2) as i32);
    assert_eq!(bounds.height, (MASK_SIDE / 2) as i32);
    let red = screen.count_in(bounds, Rgb565::RED);
    assert!(
        red > 0,
        "a box smaller than the image should scale the whole image down into it, not crop to its centre and miss the corner"
    );
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
    assert!(
        bounds.height > 40,
        "one line would be about 20 pixels, got {bounds:?}"
    );
}

// =====================================================================// Watch-wide settings
// ============================================================================

#[test]
fn settings_read_as_their_system_values() {
    use neotime_settings::{Setting, Settings};
    let mut settings = Settings::default();
    settings.set(Setting::ScreenTimeoutSeconds, 30).unwrap();
    settings.set(Setting::DoNotDisturb, 1).unwrap();

    assert_eq!(
        settings::read_setting(&settings, SystemValue::ScreenTimeoutSeconds),
        Some(30)
    );
    assert_eq!(
        settings::read_setting(&settings, SystemValue::IsDoNotDisturb),
        Some(1)
    );
    assert_eq!(
        settings::read_setting(&settings, SystemValue::StepGoal),
        Some(10_000)
    );
    assert_eq!(settings::read_setting(&settings, SystemValue::Brightness), None);
    assert_eq!(
        settings::read_setting(&settings, SystemValue::BatteryPercent),
        None
    );
}

/// An app that counts the `settings_change` events it hears.
fn settings_listener_image() -> Vec<u8> {
    use neotime_runtime::subscriptions::{Subscription, SubscriptionKind, SubscriptionValue};
    use Instruction::*;

    let mut writer = ImageWriter::new("listener", "Listener", AppKind::App, 1);
    writer.variable("changes", ValueType::Int, false, 0);
    let count_change = writer.handler(code(|code| {
        code.emit(ReadState(0))
            .emit(PushSmall(1))
            .emit(Add)
            .emit(WriteState(0));
    }));
    writer.subscription(Subscription {
        kind: SubscriptionKind::SettingsChange,
        only_while_open: true,
        handler: count_change,
        detail: 0,
        days: 0,
        value: SubscriptionValue::Constant(0),
    });
    writer.widget(WidgetKind::Column, 0, &[]);
    writer.build()
}

#[test]
fn each_changed_setting_reaches_the_open_app_as_an_event() {
    use neotime_settings::Setting;
    let image = settings_listener_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    let changed = Setting::Chimes.bit() | Setting::StepGoal.bit();
    settings::tell_app_about_changes(&mut host, changed, START_TIME, &mut system);

    assert_eq!(host.state().get(0).unwrap(), 2);
    assert!(system.failures.is_empty());
}

#[test]
fn the_new_settings_read_as_their_system_values() {
    use neotime_settings::{Setting, Settings};
    let mut settings = Settings::default();
    settings.set(Setting::Bluetooth, 0).unwrap();
    settings.set(Setting::WakeOnTouch, 2).unwrap();

    assert_eq!(
        settings::read_setting(&settings, SystemValue::IsBluetoothEnabled),
        Some(0)
    );
    assert_eq!(
        settings::read_setting(&settings, SystemValue::AllowsUpdates),
        Some(1)
    );
    assert_eq!(
        settings::read_setting(&settings, SystemValue::WakeOnTouch),
        Some(2)
    );
    assert_eq!(
        settings::read_setting(&settings, SystemValue::AlarmsInDoNotDisturb),
        Some(1)
    );
}

#[test]
fn the_wake_setting_picks_which_touch_turns_a_dark_screen_on() {
    use neotime_settings::{wake_on_touch, Setting, Settings};
    let lift = Touch::Release { x: 1, y: 2 };
    let double_tap = Touch::DoubleClick { x: 1, y: 2 };
    let press = Touch::Press { x: 1, y: 2 };
    let mut settings = Settings::default();

    assert!(settings::touch_wakes_screen(&settings, &lift));
    assert!(!settings::touch_wakes_screen(&settings, &press));
    assert!(!settings::touch_wakes_screen(&settings, &double_tap));

    settings
        .set(Setting::WakeOnTouch, wake_on_touch::DOUBLE_TAP)
        .unwrap();
    assert!(settings::touch_wakes_screen(&settings, &double_tap));
    assert!(!settings::touch_wakes_screen(&settings, &lift));

    settings.set(Setting::WakeOnTouch, wake_on_touch::OFF).unwrap();
    assert!(!settings::touch_wakes_screen(&settings, &lift));
    assert!(!settings::touch_wakes_screen(&settings, &double_tap));
}

#[test]
fn a_preset_theme_is_picked_and_read_back_by_number() {
    use neotime_runtime::PRESET_THEMES;

    let ocean = settings::preset_theme(2).unwrap();

    assert_eq!(ocean, PRESET_THEMES[2].theme);
    assert_eq!(settings::preset_number(&ocean), 2);
    assert_eq!(settings::preset_number(&DEFAULT_THEME), 0);
    assert_eq!(settings::preset_theme(PRESET_THEMES.len() as Value), None);
    assert_eq!(settings::preset_theme(-1), None);
    let mut own = DEFAULT_THEME;
    own.set_color(neotime_runtime::ThemeColor::Accent, 0x1234);
    assert_eq!(settings::preset_number(&own), -1);
}

#[test]
fn fonts_come_from_the_shared_library() {
    let library: &'static [u8] =
        std::boxed::Box::leak(neotime_compiler_library_stand_in().into_boxed_slice());
    let image = neotime_runtime::AppImage::parse(library).unwrap();
    assert_eq!(crate::fonts::install_from_library(&image), 7);
    assert!(crate::fonts::bitmap_font_bytes() > 0);
}

/// A library image holding just the seven fonts, as the compiler's builder writes them.
fn neotime_compiler_library_stand_in() -> Vec<u8> {
    let mut writer = ImageWriter::new("shared", "Shared library", AppKind::Library, 1);
    for name in ["small", "normal", "medium", "large", "huge", "giant", "condensed"] {
        let path = std::format!("{}/fonts/{name}.ntfont", env!("CARGO_MANIFEST_DIR"));
        writer.font(&std::fs::read(path).unwrap());
    }
    writer.build()
}

/// The counter app, recorded as built against shared library `version`.
fn counter_image_for_library(version: u16) -> Vec<u8> {
    let mut image = counter_image();
    image[8..10].copy_from_slice(&version.to_le_bytes());
    image
}

#[test]
fn an_app_built_for_a_newer_library_is_refused_it() {
    let library = ImageWriter::new("shared", "Shared library", AppKind::Library, 2).build();
    let mut system = FakeSystem::default();
    let open = |version: u16, system: &mut FakeSystem| {
        let image = std::boxed::Box::leak(counter_image_for_library(version).into_boxed_slice());
        let mut host = AppHost::open(image, DEFAULT_THEME, START_TIME, system).unwrap();
        host.set_library(&library, START_TIME, system).map(|_| ())
    };

    assert!(open(1, &mut system).is_ok());
    assert!(open(2, &mut system).is_ok());
    assert!(matches!(
        open(3, &mut system),
        Err(OpenError::LibraryTooOld {
            needed: 3,
            installed: 2
        })
    ));
}

#[test]
fn open_into_opens_the_app_in_the_slot() {
    let image = counter_image();
    let mut system = FakeSystem::default();
    let mut slot = None;

    let host = AppHost::open_into(&mut slot, &image, DEFAULT_THEME, &[], START_TIME, &mut system).unwrap();
    assert!(host.needs_drawing());
    assert!(slot.is_some());

    assert!(AppHost::open_into(&mut slot, &[1, 2, 3], DEFAULT_THEME, &[], START_TIME, &mut system).is_err());
    assert!(slot.is_some());
}

// ============================================================================
// System value changes
// ============================================================================

/// An app that counts the phone connecting in state 0 and going away in state 1, and keeps the steps in 2.
fn phone_listener_image() -> Vec<u8> {
    use neotime_runtime::subscriptions::{Subscription, SubscriptionKind, SubscriptionValue};
    use Instruction::*;

    let mut writer = ImageWriter::new("phone", "Phone", AppKind::App, 1);
    writer.variable("connects", ValueType::Int, false, 0);
    writer.variable("disconnects", ValueType::Int, false, 0);
    writer.variable("steps", ValueType::Int, false, 0);
    let add_one_to = |writer: &mut ImageWriter, variable: u8| {
        writer.handler(code(|code| {
            code.emit(ReadState(variable))
                .emit(PushSmall(1))
                .emit(Add)
                .emit(WriteState(variable));
        }))
    };
    let count_connect = add_one_to(&mut writer, 0);
    let count_disconnect = add_one_to(&mut writer, 1);
    let store_steps = writer.handler(code(|code| {
        code.emit(ReadEvent(0)).emit(WriteState(2));
    }));
    let subscription = |kind, handler, detail| Subscription {
        kind,
        only_while_open: true,
        handler,
        detail,
        days: 0,
        value: SubscriptionValue::Constant(0),
    };
    writer.subscription(subscription(SubscriptionKind::Phone, count_connect, 0));
    writer.subscription(subscription(SubscriptionKind::Phone, count_disconnect, 1));
    writer.subscription(subscription(SubscriptionKind::Steps, store_steps, 0));
    writer.widget(WidgetKind::Column, 0, &[]);
    writer.build()
}

#[test]
fn the_phone_connecting_and_going_away_reach_the_app_as_phone_events() {
    let image = phone_listener_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let phone_changed = SystemValue::IsPhoneConnected.bit();

    system.is_phone_connected = true;
    host.report_system_value_changes(phone_changed, START_TIME + 1, &mut system);
    assert_eq!(host.state().get(0).unwrap(), 1);
    assert_eq!(host.state().get(1).unwrap(), 0);

    system.is_phone_connected = false;
    host.report_system_value_changes(phone_changed, START_TIME + 2, &mut system);
    assert_eq!(host.state().get(0).unwrap(), 1);
    assert_eq!(host.state().get(1).unwrap(), 1);
    assert!(system.failures.is_empty());
}

#[test]
fn a_new_step_count_reaches_the_app_as_a_steps_event() {
    let image = phone_listener_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    system.steps_today = 1234;
    host.report_system_value_changes(SystemValue::StepsToday.bit(), START_TIME + 1, &mut system);

    assert_eq!(host.state().get(2).unwrap(), 1234);
    assert_eq!(host.state().get(0).unwrap(), 0);
    assert_eq!(host.state().get(1).unwrap(), 0);
}

#[test]
fn other_system_value_changes_push_no_events() {
    let image = phone_listener_image();
    let mut system = FakeSystem {
        steps_today: 99,
        is_phone_connected: true,
        ..FakeSystem::default()
    };
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    host.report_system_value_changes(SystemValue::BatteryPercent.bit(), START_TIME + 1, &mut system);

    assert_eq!(host.state().get(0).unwrap(), 0);
    assert_eq!(host.state().get(2).unwrap(), 0);
}

#[test]
fn an_image_that_names_a_library_image_draws_that_image() {
    use neotime_runtime::resources::{library_image_reference_bytes, ImageFormat, ImageReference};

    let mut library_writer = ImageWriter::new("shared", "Shared library", AppKind::Library, 1);
    library_writer.image_resource(ImageFormat::Mask, 1, 1, &[255]);
    library_writer.image_resource(ImageFormat::Mask, 2, 3, &[255; 6]);
    let library_bytes = library_writer.build();
    let library = neotime_runtime::AppImage::parse(&library_bytes).unwrap();
    let mut app_writer = ImageWriter::new("app", "App", AppKind::App, 1);
    app_writer.library_image_reference(1);
    let app_bytes = app_writer.build();
    let app = neotime_runtime::AppImage::parse(&app_bytes).unwrap();
    let state = neotime_runtime::AppState::new(&app).unwrap();
    let icon = library_image_reference_bytes(1);
    let icons: &[&[u8]] = &[&icon];
    let context = |library| crate::drawing::DrawContext {
        image: &app,
        library,
        installed_app_icons: icons,
        state: &state,
        theme: &neotime_runtime::DEFAULT_THEME,
        utc_offset: 0,
        now: 0,
        clip: crate::SCREEN_AREA,
    };
    let size_of = |context: &crate::drawing::DrawContext, reference: ImageReference| {
        context
            .image_resource(reference.to_value())
            .map(|image| (image.width, image.height))
    };

    let with_library = context(Some(&library));
    let without_library = context(None);

    assert_eq!(neotime_runtime::verify(&app), Ok(()));
    assert_eq!(size_of(&with_library, ImageReference::App(0)), Some((2, 3)));
    assert_eq!(
        size_of(&with_library, ImageReference::InstalledApp(0)),
        Some((2, 3))
    );
    assert_eq!(size_of(&without_library, ImageReference::App(0)), None);
}

// ============================================================================
// App lists
// ============================================================================

/// One AppList filling the screen, whose tap vibrates for as many milliseconds as the place tapped.
fn app_list_image() -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new("launcher", "Apps", AppKind::App, 1);
    let main = writer.string("main");
    let report_place = writer.handler(code(|code| {
        code.emit(ReadEvent(2)).emit(Effect(EffectKind::Vibrate));
    }));
    let constant = PropertySource::Constant;
    writer.widget(
        WidgetKind::AppList,
        0,
        &[
            (Property::ScreenName, constant(main as i32)),
            (Property::Padding, constant(8)),
            (Property::Gap, constant(8)),
            (Property::OnClick, PropertySource::Handler(report_place)),
        ],
    );
    writer.build()
}

fn system_with_apps(count: usize) -> FakeSystem {
    FakeSystem {
        app_names: (0..count).map(|place| std::format!("App {place}")).collect(),
        ..Default::default()
    }
}

#[test]
fn an_app_list_draws_a_row_for_each_installed_app() {
    let image = app_list_image();
    let mut system = system_with_apps(3);
    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    let rows = host.widget_rows(0);

    assert_eq!(host.node_count(), 1);
    let names: Vec<&str> = rows.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["App 0", "App 1", "App 2"]);
    assert_eq!(rows[0].1.y, 8);
    assert_eq!(rows[1].1.y - rows[0].1.y, crate::drawing::APP_ROW_HEIGHT + 8);
    assert_eq!(rows[0].1.width, 240 - 16);
}

#[test]
fn tapping_a_row_sends_its_place_and_a_gap_sends_nothing() {
    let image = app_list_image();
    let mut system = system_with_apps(3);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let second = host.widget_rows(0)[1].1;

    let on_row = host.touch(
        Touch::Click {
            x: 120,
            y: second.y + 10,
        },
        START_TIME,
        &mut system,
    );
    let in_gap = host.touch(
        Touch::Click {
            x: 120,
            y: second.y - 4,
        },
        START_TIME,
        &mut system,
    );
    let past_the_end = host.touch(Touch::Click { x: 120, y: 230 }, START_TIME, &mut system);

    assert_eq!(on_row, EventResult::Handled);
    assert_eq!(in_gap, EventResult::NotHandled);
    assert_eq!(past_the_end, EventResult::NotHandled);
    assert_eq!(system.effects, [(EffectKind::Vibrate, 1, None)]);
}

#[test]
fn a_long_app_list_scrolls_and_keeps_its_rows_inside_it() {
    let image = app_list_image();
    let mut system = system_with_apps(8);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let first_before = host.widget_rows(0)[0].1;

    host.touch(
        Touch::Drag {
            x: 120,
            y: 40,
            start_x: 120,
            start_y: 200,
        },
        START_TIME,
        &mut system,
    );
    let rows = host.widget_rows(0);

    assert!(rows[0].1.y < first_before.y);
    assert!(rows.iter().all(|(_, row)| row.y + row.height > 0 && row.y < 240));
    assert_eq!(rows.first().map(|(name, _)| *name), Some("App 2"));
    assert_eq!(rows.last().map(|(name, _)| *name), Some("App 6"));
}

#[test]
fn a_swipe_goes_through_a_list_that_did_not_move_but_not_one_that_scrolled() {
    let image = app_list_image();
    let mut system = system_with_apps(8);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let drag_down = Touch::Drag {
        x: 120,
        y: 200,
        start_x: 120,
        start_y: 40,
    };
    let drag_up = Touch::Drag {
        x: 120,
        y: 40,
        start_x: 120,
        start_y: 200,
    };

    // At the top a drag down cannot move the list, so the swipe is left to the system.
    host.touch(Touch::Press { x: 120, y: 40 }, START_TIME, &mut system);
    host.touch(drag_down, START_TIME, &mut system);
    let at_the_top = host.touch(Touch::Swipe(SwipeDirection::Down), START_TIME, &mut system);
    host.touch(Touch::Release { x: 120, y: 200 }, START_TIME, &mut system);

    host.touch(Touch::Press { x: 120, y: 200 }, START_TIME, &mut system);
    host.touch(drag_up, START_TIME, &mut system);
    let after_scrolling = host.touch(Touch::Swipe(SwipeDirection::Up), START_TIME, &mut system);

    assert_eq!(
        [at_the_top, after_scrolling],
        [EventResult::NotHandled, EventResult::Handled]
    );
}

#[test]
fn installing_an_app_adds_its_row() {
    let image = app_list_image();
    let mut system = system_with_apps(1);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    system.app_names.push("Timer".into());
    host.system_values_changed(SystemValue::InstalledAppCount.bit(), START_TIME, &mut system);

    let names: Vec<&str> = host.widget_rows(0).iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["App 0", "Timer"]);
    assert!(dirty(&host).contains(&0));
}

#[test]
fn an_app_list_paints_its_rows() {
    let image = app_list_image();
    let mut system = system_with_apps(2);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let mut screen = FakeScreen::new();

    host.draw(&mut screen).unwrap();

    let surface = rgb565(DEFAULT_THEME.color(ThemeColor::Surface));
    let background = rgb565(DEFAULT_THEME.color(ThemeColor::Background));
    assert_eq!(screen.pixel(30, 12), surface);
    assert_eq!(screen.pixel(30, 80), surface);
    assert_eq!(screen.pixel(30, 64), background);
    assert_eq!(screen.pixel(30, 200), background);
}

/// An AppList with the extra properties given, whose `first` follows a state variable a left swipe sets to 2.
fn app_list_image_with(extra: &[(Property, PropertySource)]) -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new("launcher", "Apps", AppKind::App, 1);
    writer.variable("first", ValueType::Int, false, 0);
    let main = writer.string("main");
    let first = writer.expression(code(|code| {
        code.emit(ReadState(0));
    }));
    let next_page = writer.handler(code(|code| {
        code.emit(PushSmall(2)).emit(WriteState(0));
    }));
    let report_place = writer.handler(code(|code| {
        code.emit(ReadEvent(2)).emit(Effect(EffectKind::Vibrate));
    }));
    let constant = PropertySource::Constant;
    let mut properties = std::vec![
        (Property::ScreenName, constant(main as i32)),
        (Property::Padding, constant(8)),
        (Property::Gap, constant(8)),
        (Property::FirstApp, PropertySource::Expression(first)),
        (Property::OnClick, PropertySource::Handler(report_place)),
        (Property::OnSwipe, PropertySource::Handler(next_page)),
    ];
    properties.extend_from_slice(extra);
    writer.widget(WidgetKind::AppList, 0, &properties);
    writer.build()
}

fn row_names(host: &AppHost) -> Vec<String> {
    host.widget_rows(0)
        .iter()
        .map(|(name, _)| String::from(*name))
        .collect()
}

#[test]
fn an_app_list_shows_only_its_run_of_apps_and_reports_their_places() {
    let image = app_list_image_with(&[(Property::AppCount, PropertySource::Constant(2))]);
    let mut system = system_with_apps(5);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    assert_eq!(row_names(&host), ["App 0", "App 1"]);

    host.touch(Touch::Swipe(SwipeDirection::Left), START_TIME, &mut system);
    let second = host.widget_rows(0)[1].1;
    host.touch(
        Touch::Click {
            x: 120,
            y: second.y + 10,
        },
        START_TIME,
        &mut system,
    );

    assert_eq!(row_names(&host), ["App 2", "App 3"]);
    assert_eq!(system.effects, [(EffectKind::Vibrate, 3, None)]);
}

#[test]
fn a_run_past_the_last_app_stops_at_it() {
    let image = app_list_image_with(&[(Property::AppCount, PropertySource::Constant(4))]);
    let mut system = system_with_apps(3);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    host.touch(Touch::Swipe(SwipeDirection::Left), START_TIME, &mut system);

    assert_eq!(row_names(&host), ["App 2"]);
}

#[test]
fn an_app_list_with_sections_heads_each_group_and_skips_headings_when_tapped() {
    let image = app_list_image_with(&[(Property::Sections, PropertySource::Constant(1))]);
    let mut system = system_with_apps(3);
    system.groups = std::vec![(String::from("Games"), 0), (String::from("Tools"), 2)];
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    let rows = host.widget_rows(0);
    assert_eq!(row_names(&host), ["Games", "App 0", "App 1", "Tools", "App 2"]);
    assert_eq!(rows[0].1.height, crate::drawing::APP_HEADING_HEIGHT);

    let (heading, last_app) = (rows[3].1, rows[4].1);
    let on_heading = host.touch(
        Touch::Click {
            x: 120,
            y: heading.y + 4,
        },
        START_TIME,
        &mut system,
    );
    host.touch(
        Touch::Click {
            x: 120,
            y: last_app.y + 4,
        },
        START_TIME,
        &mut system,
    );

    assert_eq!(on_heading, EventResult::NotHandled);
    assert_eq!(system.effects, [(EffectKind::Vibrate, 2, None)]);
}

#[test]
fn an_app_list_in_columns_lays_its_apps_out_as_square_cells() {
    let image = app_list_image_with(&[
        (Property::Columns, PropertySource::Constant(3)),
        (Property::Sections, PropertySource::Constant(1)),
    ]);
    let mut system = system_with_apps(5);
    system.groups = std::vec![(String::from("Games"), 0), (String::from("Tools"), 4)];
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    let rows = host.widget_rows(0);
    let cell = rows[1].1;
    // 240 less 16 of padding, 4 kept for the scroll bar and two 8 pixel gaps, in three columns.
    assert_eq!((cell.width, cell.height), (68, 68));
    assert_eq!(rows[2].1.x - cell.x, 76);
    assert_eq!(rows[4].1.y - cell.y, 76);
    // The second group's heading starts a row of its own, under the half-full one.
    assert_eq!(rows[5].1.y, rows[4].1.y + 76);
    // Its one app sits below it, past the screen's bottom.
    assert_eq!(rows.len(), 6);

    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();
    let surface = rgb565(DEFAULT_THEME.color(ThemeColor::Surface));
    assert_eq!(screen.pixel(cell.x + 2, cell.y + cell.height / 2), surface);
}

#[test]
fn a_tall_grid_scrolls_by_its_rows() {
    let image = app_list_image_with(&[(Property::Columns, PropertySource::Constant(3))]);
    let mut system = system_with_apps(15);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    // Five rows of 69 with their gaps are taller than the screen, so the last is out of sight.
    assert_eq!(host.widget_rows(0).len(), 12);

    host.touch(
        Touch::Drag {
            x: 120,
            y: 40,
            start_x: 120,
            start_y: 200,
        },
        START_TIME,
        &mut system,
    );

    assert_eq!(row_names(&host).last().map(String::as_str), Some("App 14"));
}

// ============================================================================
// Text files
// ============================================================================

/// One TextFile filling the screen, showing `/notes/note.txt` in the font `/notes/font` names.
fn text_file_image() -> Vec<u8> {
    let mut writer = ImageWriter::new("notes", "Notes", AppKind::App, 1);
    let main = writer.string("main");
    let path = writer.string("/notes/note.txt");
    let font_path = writer.string("/notes/font");
    let placeholder = writer.string("No note yet");
    let constant = PropertySource::Constant;
    writer.widget(
        WidgetKind::TextFile,
        0,
        &[
            (Property::ScreenName, constant(main as i32)),
            (Property::Padding, constant(8)),
            (Property::Path, constant(path as i32)),
            (Property::FontPath, constant(font_path as i32)),
            (Property::Placeholder, constant(placeholder as i32)),
        ],
    );
    writer.build()
}

/// A note of numbered lines, each short enough not to wrap.
fn numbered_note(lines: usize) -> Vec<u8> {
    (0..lines)
        .map(|line| std::format!("Line {line}\n"))
        .collect::<String>()
        .into_bytes()
}

fn system_with_note(note: Vec<u8>, font: Option<&str>) -> FakeSystem {
    let mut files = alloc::vec![(String::from("/notes/note.txt"), note)];
    if let Some(font) = font {
        files.push((String::from("/notes/font"), font.as_bytes().to_vec()));
    }
    FakeSystem {
        files,
        ..Default::default()
    }
}

fn shown_lines(host: &AppHost) -> Vec<String> {
    host.widget_rows(0)
        .iter()
        .map(|(line, _)| String::from(*line))
        .collect()
}

#[test]
fn a_text_file_shows_its_first_lines_wrapped_to_the_screen() {
    let image = text_file_image();
    let note =
        b"A note long enough that its words cannot all fit across one line of the watch.\nSecond".to_vec();
    let mut system = system_with_note(note, None);
    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    let rows = host.widget_rows(0);

    assert!(rows.len() >= 3);
    assert!(rows.iter().all(|(_, row)| row.width == 240 - 16 && row.x == 8));
    assert_eq!(rows[0].1.y, 8);
    assert_eq!(rows.last().map(|(line, _)| *line), Some("Second"));
    assert!(system.failures.is_empty());
}

#[test]
fn the_font_file_picks_the_font_and_its_line_height() {
    let image = text_file_image();
    let mut system = system_with_note(numbered_note(40), Some("large\n"));
    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let rows = host.widget_rows(0);
    let large = crate::fonts::line_height(FontName::Large.metrics());
    assert_eq!(rows[1].1.y - rows[0].1.y, large);

    let mut system = system_with_note(numbered_note(40), Some("nonsense"));
    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let rows = host.widget_rows(0);
    let normal = crate::fonts::line_height(FontName::Normal.metrics());
    assert_eq!(rows[1].1.y - rows[0].1.y, normal);
}

#[test]
fn a_long_text_file_scrolls_to_its_last_line_holding_only_lines_near_the_view() {
    let image = text_file_image();
    let mut system = system_with_note(numbered_note(500), Some("small"));
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    assert_eq!(shown_lines(&host).first().map(String::as_str), Some("Line 0"));
    assert!(host.widget_text(0).unwrap().len() < 1024);

    for _ in 0..200 {
        host.touch(
            Touch::Drag {
                x: 120,
                y: 0,
                start_x: 120,
                start_y: 239,
            },
            START_TIME,
            &mut system,
        );
        host.touch(Touch::Release { x: 120, y: 0 }, START_TIME, &mut system);
    }

    let lines = shown_lines(&host);
    assert_eq!(lines.last().map(String::as_str), Some("Line 499"));
    assert!(host
        .widget_rows(0)
        .iter()
        .all(|(_, row)| row.y + row.height > 0 && row.y < 240));
    assert!(host.widget_text(0).unwrap().len() < 1024);
}

#[test]
fn a_missing_or_empty_text_file_shows_its_placeholder_in_the_muted_colour() {
    let image = text_file_image();
    for mut system in [FakeSystem::default(), system_with_note(Vec::new(), None)] {
        let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
        let mut screen = FakeScreen::new();

        host.draw(&mut screen).unwrap();

        assert_eq!(shown_lines(&host), ["No note yet"]);
        assert!(system.failures.is_empty());
        let muted = rgb565(DEFAULT_THEME.color(ThemeColor::Muted));
        let height = crate::fonts::line_height(FontName::Normal.metrics());
        assert!((8..120).any(|x| (8..8 + height).any(|y| screen.pixel(x, y) == muted)));
    }
}

#[test]
fn a_text_file_paints_its_letters() {
    let image = text_file_image();
    let mut system = system_with_note(b"MMMM".to_vec(), None);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let mut screen = FakeScreen::new();

    host.draw(&mut screen).unwrap();

    let background = rgb565(DEFAULT_THEME.color(ThemeColor::Background));
    let height = crate::fonts::line_height(FontName::Normal.metrics());
    let inked = (8..40).any(|x| (8..8 + height).any(|y| screen.pixel(x, y) != background));
    assert!(inked);
    assert!((8..40).all(|x| screen.pixel(x, 8 + 2 * height) == background));
}

#[test]
fn a_text_file_draws_only_the_lines_inside_the_part_being_painted() {
    use neotime_ui::geometry::{Color, Rect};
    use neotime_ui::renderer::Renderer;

    /// Records the top of everything drawn.
    #[derive(Default)]
    struct Tops(Vec<i32>);
    impl Renderer for Tops {
        fn fill_rect(&mut self, rect: Rect, _color: Color) {
            self.0.push(rect.y);
        }
        fn blend_rect(&mut self, rect: Rect, _color: Color) {
            self.0.push(rect.y);
        }
        fn draw_text(&mut self, _position: (i32, i32), _text: &str, _color: Color) {}
    }

    let bytes = ImageWriter::new("notes", "Notes", AppKind::App, 1).build();
    let image = neotime_runtime::AppImage::parse(&bytes).unwrap();
    let state = neotime_runtime::AppState::new(&image).unwrap();
    let mut node = crate::nodes::Node::new(0, WidgetKind::TextFile, 1, None, SCREEN_AREA);
    node.shown = true;
    node.text = (0..12)
        .map(|line| std::format!("MMMM {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(extra) = node.extra.get_mut() {
        *extra = crate::nodes::Extra::TextFile {
            line_count: 12,
            first_line: 0,
            held_lines: 12,
            is_placeholder: false,
        };
    }
    let height = crate::fonts::line_height(FontName::Normal.metrics());
    let strip = Rect {
        x: 0,
        y: 3 * height,
        width: 240,
        height: 4,
    };
    let context = crate::drawing::DrawContext {
        image: &image,
        library: None,
        installed_app_icons: &[],
        state: &state,
        theme: &neotime_runtime::DEFAULT_THEME,
        utc_offset: 0,
        now: 0,
        clip: strip,
    };
    let mut tops = Tops::default();

    crate::drawing::draw_node(&node, &context, &mut tops);

    assert!(!tops.0.is_empty());
    assert!(
        tops.0.iter().all(|top| (3 * height..4 * height).contains(top)),
        "{:?}",
        tops.0
    );
}

// ============================================================================
// Hidden widgets
// ============================================================================

/// A button that flips a flag, and a panel of two labels shown only while the flag is set.
fn hidden_panel_image() -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new("panel", "Panel", AppKind::App, 1);
    let is_open = writer.variable("is_open", ValueType::Bool, false, 0);
    let main = writer.string("main");
    let first = writer.string("First line");
    let second = writer.string("Second line");
    let toggle_text = writer.string("Toggle");
    let flip = writer.handler(code(|code| {
        code.emit(ReadState(is_open)).emit(Not).emit(WriteState(is_open));
    }));
    let shown = writer.expression(code(|code| {
        code.emit(ReadState(is_open));
    }));
    let constant = PropertySource::Constant;
    writer.widget(
        WidgetKind::Column,
        2,
        &[(Property::ScreenName, constant(main as i32))],
    );
    writer.widget(
        WidgetKind::Button,
        0,
        &[
            (Property::Text, constant(toggle_text as i32)),
            (Property::Height, constant(BUTTON_HEIGHT)),
            (Property::OnClick, PropertySource::Handler(flip)),
        ],
    );
    writer.widget(
        WidgetKind::Column,
        2,
        &[(Property::Visible, PropertySource::Expression(shown))],
    );
    writer.widget(WidgetKind::Label, 0, &[(Property::Text, constant(first as i32))]);
    writer.widget(WidgetKind::Label, 0, &[(Property::Text, constant(second as i32))]);
    writer.build()
}

const PANEL: u16 = 2;
const PANEL_FIRST_LINE: u16 = 3;

#[test]
fn a_hidden_widget_holds_no_nodes_for_what_is_inside_it() {
    let image = hidden_panel_image();
    let mut system = FakeSystem::default();

    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    assert_eq!(host.node_count(), 3);
    assert_eq!(host.widget_bounds(PANEL).map(|(_, shown)| shown), Some(false));
    assert_eq!(host.widget_text(PANEL_FIRST_LINE), None);
}

#[test]
fn showing_a_hidden_widget_builds_its_nodes_and_hiding_it_frees_them() {
    let image = hidden_panel_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();
    let text_color = rgb565(DEFAULT_THEME.color(ThemeColor::Text));

    click_on(&mut host, 1, START_TIME, &mut system);
    host.draw(&mut screen).unwrap();
    let (line, shown) = host.widget_bounds(PANEL_FIRST_LINE).unwrap();
    let drawn_while_open = screen.count_in(line, text_color);
    let nodes_while_open = host.node_count();

    click_on(&mut host, 1, START_TIME, &mut system);
    assert!(host.needs_drawing());
    host.draw(&mut screen).unwrap();

    assert!(shown);
    assert_eq!(nodes_while_open, 5);
    assert!(drawn_while_open > 20);
    assert_eq!(host.node_count(), 3);
    assert_eq!(screen.count_in(line, text_color), 0);
    assert!(system.failures.is_empty());
}

// ============================================================================
// Repeated rows
// ============================================================================

/// A Scroll of 12 repeated rows, each a button reading its row number, shown while it is below `count`.
///
/// A tap on a row vibrates for as many milliseconds as its row; the button under the list sets `count` to 2.
fn repeated_rows_image(row_height: Option<i32>) -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new("rows", "Rows", AppKind::App, 1);
    let count = writer.variable("count", ValueType::Int, false, 12);
    let main = writer.string("main");
    let fewer = writer.string("Fewer");
    let row_text = writer.expression(code(|code| {
        code.emit(ReadRow).emit(FormatNumber(1));
    }));
    let row_shows = writer.expression(code(|code| {
        code.emit(ReadRow).emit(ReadState(count)).emit(Less);
    }));
    let report_row = writer.handler(code(|code| {
        code.emit(ReadRow).emit(Effect(EffectKind::Vibrate));
    }));
    let show_two = writer.handler(code(|code| {
        code.emit(PushSmall(2)).emit(WriteState(count));
    }));
    let constant = PropertySource::Constant;
    writer.widget(
        WidgetKind::Column,
        2,
        &[(Property::ScreenName, constant(main as i32))],
    );
    writer.widget(
        WidgetKind::Scroll,
        1,
        &[(Property::Grow, constant(1)), (Property::Gap, constant(4))],
    );
    writer.widget(
        WidgetKind::Repeat,
        1,
        &[(Property::RowCount, constant(12)), (Property::Gap, constant(4))],
    );
    let mut row = alloc::vec![
        (Property::Text, PropertySource::Expression(row_text)),
        (Property::Visible, PropertySource::Expression(row_shows)),
        (Property::OnClick, PropertySource::Handler(report_row)),
    ];
    if let Some(height) = row_height {
        row.push((Property::Height, constant(height)));
    }
    writer.widget(WidgetKind::Button, 0, &row);
    writer.widget(
        WidgetKind::Button,
        0,
        &[
            (Property::Text, constant(fewer as i32)),
            (Property::Height, constant(BUTTON_HEIGHT)),
            (Property::OnClick, PropertySource::Handler(show_two)),
        ],
    );
    writer.build()
}

const ROW: u16 = 3;
const FEWER_BUTTON: u16 = 4;

/// The rows that have nodes, by the text each shows.
fn built_rows(host: &AppHost) -> Vec<String> {
    host.widget_texts(ROW).into_iter().map(String::from).collect()
}

#[test]
fn only_the_rows_in_view_are_built() {
    let image = repeated_rows_image(Some(40));
    let mut system = FakeSystem::default();

    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    let rows = built_rows(&host);
    assert_eq!(rows.first().map(String::as_str), Some("0"));
    assert!(rows.len() < 12, "{rows:?}");
    assert!(host.node_count() < 12);
    assert!(system.failures.is_empty());
}

#[test]
fn scrolling_builds_the_rows_that_come_into_view_and_drops_the_rest() {
    let image = repeated_rows_image(Some(40));
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let nodes_at_the_top = host.node_count();

    host.touch(
        Touch::Drag {
            x: 120,
            y: 0,
            start_x: 120,
            start_y: 200,
        },
        START_TIME,
        &mut system,
    );
    host.touch(
        Touch::Drag {
            x: 120,
            y: -400,
            start_x: 120,
            start_y: 200,
        },
        START_TIME,
        &mut system,
    );

    let rows = built_rows(&host);
    assert_eq!(rows.last().map(String::as_str), Some("11"));
    assert!(!rows.contains(&"0".to_string()), "{rows:?}");
    assert!(host.node_count() <= nodes_at_the_top + 1);
}

#[test]
fn a_tap_on_a_row_runs_its_handler_for_that_row() {
    let image = repeated_rows_image(Some(40));
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    host.touch(Touch::Click { x: 120, y: 44 + 20 }, START_TIME, &mut system);

    assert_eq!(system.effects, [(EffectKind::Vibrate, 1, None)]);
}

#[test]
fn rows_that_stop_showing_give_up_their_slots() {
    let image = repeated_rows_image(Some(40));
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    click_on(&mut host, FEWER_BUTTON, START_TIME, &mut system);

    assert_eq!(built_rows(&host), ["0", "1"]);
    let (fewer, _) = host.widget_bounds(FEWER_BUTTON).unwrap();
    assert_eq!(fewer.y, 240 - BUTTON_HEIGHT);
}

#[test]
fn rows_of_no_fixed_height_are_all_built() {
    let image = repeated_rows_image(None);
    let mut system = FakeSystem::default();

    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    assert_eq!(built_rows(&host).len(), 12);
}

#[test]
fn the_rows_in_view_are_worked_out_from_the_slots() {
    use crate::nodes::slots_in_view;
    let clip = Rect {
        x: 0,
        y: 0,
        width: 240,
        height: 100,
    };

    assert_eq!(slots_in_view(0, clip, 40, 4, 12), (0, 3));
    assert_eq!(slots_in_view(-44, clip, 40, 4, 12), (1, 4));
    assert_eq!(slots_in_view(-40, clip, 40, 4, 12), (1, 4));
    assert_eq!(slots_in_view(-39, clip, 40, 4, 12), (0, 4));
    assert_eq!(slots_in_view(-1000, clip, 40, 4, 12), (12, 12));
    assert_eq!(slots_in_view(0, clip, 40, 4, 2), (0, 2));
}

// ============================================================================
// Batteries, status icons and badges
// ============================================================================

/// A Battery, a StatusIcons and a Badge stacked in a column, each with the properties given.
fn status_image(battery: &[(Property, i32)], status: &[(Property, i32)], badge_text: &str) -> Vec<u8> {
    let mut writer = ImageWriter::new("status", "Status", AppKind::App, 1);
    let main = writer.string("main");
    let text = writer.string(badge_text);
    let constant = |pairs: &[(Property, i32)]| -> Vec<(Property, PropertySource)> {
        pairs
            .iter()
            .map(|(property, value)| (*property, PropertySource::Constant(*value)))
            .collect()
    };
    writer.widget(
        WidgetKind::Column,
        3,
        &[(Property::ScreenName, PropertySource::Constant(main as i32))],
    );
    writer.widget(WidgetKind::Battery, 0, &constant(battery));
    writer.widget(WidgetKind::StatusIcons, 0, &constant(status));
    let mut badge = constant(&[(Property::Icon, 0x8000 + 39), (Property::Gap, 5)]);
    badge.push((Property::Text, PropertySource::Constant(text as i32)));
    writer.widget(WidgetKind::Badge, 0, &badge);
    writer.build()
}

const BATTERY_WIDGET: u16 = 1;
const STATUS_WIDGET: u16 = 2;
const BADGE_WIDGET: u16 = 3;

fn widget_size(host: &AppHost, widget: u16) -> (i32, i32) {
    let (bounds, _) = host.widget_bounds(widget).unwrap();
    (bounds.width, bounds.height)
}

#[test]
fn a_battery_is_as_wide_as_its_parts() {
    let plain = status_image(&[(Property::Value, 80), (Property::Align, 0)], &[], "");
    let charging = status_image(
        &[
            (Property::Value, 80),
            (Property::Charging, 1),
            (Property::ShowPercent, 1),
            (Property::Align, 0),
        ],
        &[],
        "",
    );
    let mut system = FakeSystem::default();

    let plain = AppHost::open(&plain, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let charging = AppHost::open(&charging, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    assert_eq!(widget_size(&plain, BATTERY_WIDGET).1, 14);
    assert!(widget_size(&charging, BATTERY_WIDGET).1 >= 16);
    assert_eq!(widget_size(&plain, BATTERY_WIDGET).0, 28);
    assert!(widget_size(&charging, BATTERY_WIDGET).0 > 28 + 16);
    assert!(system.failures.is_empty());
}

#[test]
fn status_icons_grow_with_the_icons_they_show() {
    let none = status_image(&[], &[(Property::Value, 50), (Property::Align, 0)], "");
    let all = status_image(
        &[],
        &[
            (Property::Value, 50),
            (Property::StatusFlags, 7),
            (Property::IconSize, 22),
            (Property::Gap, 6),
            (Property::Align, 0),
        ],
        "",
    );
    let mut system = FakeSystem::default();

    let none = AppHost::open(&none, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let all = AppHost::open(&all, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    assert_eq!(widget_size(&none, STATUS_WIDGET).1, 14);
    assert_eq!(widget_size(&all, STATUS_WIDGET).1, 22);
}

#[test]
fn a_low_battery_fills_in_the_danger_colour_and_a_charging_one_in_success() {
    let mut screen = FakeScreen::new();
    let mut system = FakeSystem::default();
    let danger = rgb565(DEFAULT_THEME.color(ThemeColor::Danger));
    let success = rgb565(DEFAULT_THEME.color(ThemeColor::Success));

    let low = status_image(&[(Property::Value, 10)], &[], "");
    let mut host = AppHost::open(&low, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.draw(&mut screen).unwrap();
    let (battery, _) = host.widget_bounds(BATTERY_WIDGET).unwrap();
    let low_danger = screen.count_in(battery, danger);

    let charging = status_image(&[(Property::Value, 90), (Property::Charging, 1)], &[], "");
    let mut host = AppHost::open(&charging, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();
    let (battery, _) = host.widget_bounds(BATTERY_WIDGET).unwrap();

    assert!(low_danger > 0);
    assert!(screen.count_in(battery, success) > 100);
}

#[test]
fn a_badge_shows_its_text_beside_its_icon() {
    let image = status_image(&[], &[], "1234");
    let mut system = FakeSystem::default();
    let mut screen = FakeScreen::new();

    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.draw(&mut screen).unwrap();

    let (width, height) = widget_size(&host, BADGE_WIDGET);
    assert_eq!(host.widget_text(BADGE_WIDGET), Some("1234"));
    assert!(height >= 20);
    assert!(width > 20);
    let (badge, _) = host.widget_bounds(BADGE_WIDGET).unwrap();
    assert!(screen.count_in(badge, rgb565(DEFAULT_THEME.color(ThemeColor::Text))) > 20);
}

// ============================================================================
// Pickers and Confirm
// ============================================================================

/// One picker filling the screen, whose `onChange` vibrates for as many milliseconds as the value it sends.
fn picker_image(kind: WidgetKind, properties: &[(Property, i32)], text: &str) -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new("picker", "Picker", AppKind::App, 1);
    let main = writer.string("main");
    let text = writer.string(text);
    let report = writer.handler(code(|code| {
        code.emit(ReadEvent(0)).emit(Effect(EffectKind::Vibrate));
    }));
    let mut all: Vec<(Property, PropertySource)> = properties
        .iter()
        .map(|(property, value)| (*property, PropertySource::Constant(*value)))
        .collect();
    all.push((Property::ScreenName, PropertySource::Constant(main as i32)));
    all.push((Property::OnChange, PropertySource::Handler(report)));
    if kind == WidgetKind::Stepper || kind == WidgetKind::Confirm {
        all.push((Property::Text, PropertySource::Constant(text as i32)));
    }
    writer.widget(kind, 0, &all);
    writer.build()
}

/// What tapping a picker at a point sends, or none when the tap is not handled.
fn sent_by_tap(image: &[u8], x: i32, y: i32) -> Option<Value> {
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();
    host.touch(Touch::Click { x, y }, START_TIME, &mut system);
    assert!(system.failures.is_empty());
    system.effects.first().map(|(_, value, _)| *value)
}

#[test]
fn time_rollers_roll_the_tapped_column_and_wrap_into_the_minute_of_the_day() {
    let image = picker_image(WidgetKind::TimeRollers, &[(Property::Value, 8 * 60 + 30)], "");
    let mut system = FakeSystem::default();
    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let (bounds, _) = host.widget_bounds(0).unwrap();
    let row_height = crate::drawing::option_row_height_of_font(crate::fonts::FontName::Normal);
    let middle = bounds.y + bounds.height / 2;
    let (width, _) = (bounds.width, 0);

    // The hour column is left of the middle, the minute column right of it; a row lower is one more.
    let hour_x = bounds.x + width / 2 - 40;
    let minute_x = bounds.x + width / 2 + 40;

    assert_eq!(
        sent_by_tap(&image, hour_x, middle + row_height),
        Some(9 * 60 + 30)
    );
    assert_eq!(
        sent_by_tap(&image, minute_x, middle - row_height),
        Some(8 * 60 + 29)
    );
    assert_eq!(sent_by_tap(&image, bounds.x + 2, middle), None);
}

#[test]
fn twelve_hour_time_rollers_switch_between_morning_and_afternoon() {
    let image = picker_image(
        WidgetKind::TimeRollers,
        &[(Property::Value, 8 * 60), (Property::TwelveHour, 1)],
        "",
    );
    let mut system = FakeSystem::default();
    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let (bounds, _) = host.widget_bounds(0).unwrap();
    // The AM and PM button is the rightmost part, 56 pixels wide, with the rest centred beside it.
    let digits = crate::fonts::text_width(crate::fonts::FontName::Normal.metrics(), "00") + 4 * 8;
    let colon = crate::fonts::text_width(crate::fonts::FontName::Normal.metrics(), ":");
    let width = 2 * digits + colon + 2 * 6 + 6 + 56;
    let toggle_x = bounds.x + (bounds.width - width) / 2 + width - 20;

    assert_eq!(
        sent_by_tap(&image, toggle_x, bounds.y + bounds.height / 2),
        Some(20 * 60)
    );
}

#[test]
fn duration_rollers_send_seconds() {
    let image = picker_image(WidgetKind::DurationRollers, &[(Property::Value, 5 * 60 + 7)], "");
    let mut system = FakeSystem::default();
    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let (bounds, _) = host.widget_bounds(0).unwrap();
    let row_height = crate::drawing::option_row_height_of_font(crate::fonts::FontName::Normal);
    let middle = bounds.y + bounds.height / 2;

    assert_eq!(
        sent_by_tap(&image, bounds.x + bounds.width / 2 + 40, middle + row_height),
        Some(5 * 60 + 8)
    );
    assert_eq!(
        sent_by_tap(&image, bounds.x + bounds.width / 2 - 40, middle + row_height),
        Some(6 * 60 + 7)
    );
}

#[test]
fn a_number_roller_sends_the_number_tapped() {
    let image = picker_image(
        WidgetKind::NumberRoller,
        &[
            (Property::Value, 6),
            (Property::Minimum, 2),
            (Property::Maximum, 99),
            (Property::Digits, 2),
        ],
        "",
    );
    let mut system = FakeSystem::default();
    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let (bounds, _) = host.widget_bounds(0).unwrap();
    let row_height = crate::drawing::option_row_height_of_font(crate::fonts::FontName::Normal);

    assert_eq!(
        sent_by_tap(&image, 120, bounds.y + bounds.height / 2 - row_height),
        Some(5)
    );
}

#[test]
fn a_day_button_toggles_its_day() {
    let image = picker_image(
        WidgetKind::WeekdayPicker,
        &[(Property::Value, 0b0000_0011), (Property::Columns, 7)],
        "",
    );

    // Seven buttons across the screen: the first is Monday, the third Wednesday.
    assert_eq!(sent_by_tap(&image, 240 / 14, 120), Some(0b0000_0010));
    assert_eq!(sent_by_tap(&image, 5 * 240 / 14, 120), Some(0b0000_0111));
}

#[test]
fn a_stepper_steps_within_its_limits() {
    let image = picker_image(
        WidgetKind::Stepper,
        &[
            (Property::Value, 10),
            (Property::Minimum, 0),
            (Property::Maximum, 12),
            (Property::Step, 5),
        ],
        "",
    );

    assert_eq!(sent_by_tap(&image, 5, 120), Some(5));
    assert_eq!(sent_by_tap(&image, 235, 120), Some(12));
    assert_eq!(sent_by_tap(&image, 120, 120), None);
}

/// How many text-coloured pixels a large-font Stepper showing `value` draws over its buttons, laid out as
/// the settings app's step goal.
fn stepper_text_pixels_on_buttons(value: i32) -> usize {
    let properties = [
        (Property::Value, value),
        (Property::Minimum, 1000),
        (Property::Maximum, 20000),
        (Property::Width, 224),
        (Property::Height, 52),
        (Property::Gap, 6),
        (Property::Font, 3),
    ];
    let image = picker_image(WidgetKind::Stepper, &properties, "");
    let mut system = FakeSystem::default();
    let mut screen = FakeScreen::new();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.draw(&mut screen).unwrap();
    let (bounds, _) = host.widget_bounds(0).unwrap();
    let surface = rgb565(DEFAULT_THEME.color(ThemeColor::Surface));
    let text = rgb565(DEFAULT_THEME.color(ThemeColor::Text));
    let rows = bounds.y..bounds.y + bounds.height;

    let is_button_column = |x: i32| rows.clone().any(|y| screen.pixel(x, y) == surface);
    (bounds.x..bounds.x + bounds.width)
        .filter(|&x| is_button_column(x))
        .map(|x| rows.clone().filter(|&y| screen.pixel(x, y) == text).count())
        .sum()
}

#[test]
fn a_stepper_keeps_its_widest_value_off_its_buttons() {
    let signs_only = stepper_text_pixels_on_buttons(1000);

    assert_eq!(stepper_text_pixels_on_buttons(10000), signs_only);
}

#[test]
fn a_confirm_sends_one_for_its_confirm_button_and_zero_for_the_other() {
    let image = picker_image(WidgetKind::Confirm, &[], "Keep\nClear\nClear the picture?");
    let mut system = FakeSystem::default();
    let mut screen = FakeScreen::new();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    host.draw(&mut screen).unwrap();
    let danger = rgb565(DEFAULT_THEME.color(ThemeColor::Danger));
    // The question wraps onto two lines, so the buttons start below it at about 82.
    let button_row = 100;

    assert_eq!(sent_by_tap(&image, 60, button_row), Some(0));
    assert_eq!(sent_by_tap(&image, 180, button_row), Some(1));
    assert_eq!(sent_by_tap(&image, 120, 230), None);
    assert!(
        screen.count_in(
            Rect {
                x: 124,
                y: 0,
                width: 116,
                height: 240
            },
            danger
        ) > 500
    );
}

#[test]
fn a_derive_reads_the_arrays_native_work_reads_but_not_the_one_it_writes() {
    use neotime_runtime::instruction::{ArrayQuestion, ArrayWork};
    use Instruction::*;

    let mut writer = ImageWriter::new("arrays", "Arrays", AppKind::App, 1);
    let asked = writer.array("asked", ValueType::Int, false, 4, 1, 0);
    let written = writer.array("written", ValueType::Int, false, 4, 1, 0);
    let source = writer.array("source", ValueType::Int, false, 4, 1, 0);
    let compared = writer.array("compared", ValueType::Int, false, 4, 1, 0);
    let mut code = neotime_runtime::writer::CodeWriter::new();
    code.emit(PushSmall(0))
        .emit(AskArray {
            question: ArrayQuestion::CountOf,
            array: asked,
        })
        .emit(Drop)
        .emit(WorkArray {
            work: ArrayWork::Equal,
            destination: compared,
            source: compared,
        })
        .emit(PushSmall(1))
        .emit(WorkArray {
            work: ArrayWork::PushLast,
            destination: written,
            source,
        });
    code.emit(Drop);
    writer.handler(code.finish().unwrap());
    let bytes = writer.build();
    let image = neotime_runtime::AppImage::parse(&bytes).unwrap();

    let inputs = crate::subscriptions::derive_inputs(&image, 0);

    assert_eq!(inputs.state_slots, (1 << asked) | (1 << source) | (1 << compared));
}

// ============================================================================
// Pages
// ============================================================================

/// Three pages, the middle one a list taller than the screen; turning a page keeps the new page's number.
fn pages_image(start_page: i32) -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new("pages", "Pages", AppKind::App, 1);
    let page = writer.variable("page", ValueType::Int, false, 0);
    let main = writer.string("main");
    let first = writer.string("First");
    let row = writer.string("Row");
    let last = writer.string("Last");
    let keep_page = writer.handler(code(|code| {
        code.emit(ReadEvent(0)).emit(WriteState(page));
    }));
    let constant = PropertySource::Constant;
    writer.widget(
        WidgetKind::Column,
        1,
        &[(Property::ScreenName, constant(main as i32))],
    );
    writer.widget(
        WidgetKind::Pages,
        3,
        &[
            (Property::Grow, constant(1)),
            (Property::Value, constant(start_page)),
            (Property::OnPageChange, PropertySource::Handler(keep_page)),
        ],
    );
    writer.widget(WidgetKind::Column, 1, &[(Property::Grow, constant(1))]);
    writer.widget(WidgetKind::Label, 0, &[(Property::Text, constant(first as i32))]);
    writer.widget(
        WidgetKind::Scroll,
        LIST_ROWS as u8,
        &[(Property::Grow, constant(1))],
    );
    for _ in 0..LIST_ROWS {
        writer.widget(
            WidgetKind::Label,
            0,
            &[
                (Property::Text, constant(row as i32)),
                (Property::Height, constant(60)),
            ],
        );
    }
    writer.widget(WidgetKind::Column, 1, &[(Property::Grow, constant(1))]);
    writer.widget(WidgetKind::Label, 0, &[(Property::Text, constant(last as i32))]);
    writer.build()
}

const LIST_ROWS: u16 = 6;
const PAGES: u16 = 1;
const FIRST_LABEL: u16 = 3;
const LIST: u16 = 4;
const LAST_LABEL: u16 = 6 + LIST_ROWS;

/// Where the first row of the pages' list lies.
fn row_areas(host: &AppHost) -> Option<Rect> {
    host.widget_bounds(LIST + 1).map(|(area, _)| area)
}

#[test]
fn only_the_page_on_show_is_built_and_it_fills_the_pages_above_the_bar() {
    let image = pages_image(0);
    let mut system = FakeSystem::default();
    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    assert_eq!(host.widget_text(FIRST_LABEL), Some("First"));
    assert_eq!(host.widget_bounds(LIST), None);
    assert_eq!(host.widget_bounds(LAST_LABEL), None);
    let (pages, _) = host.widget_bounds(PAGES).unwrap();
    let (page, _) = host.widget_bounds(FIRST_LABEL - 1).unwrap();
    assert_eq!(page.height, pages.height - crate::drawing::SCROLL_BAR_ROOM);
}

#[test]
fn a_swipe_left_turns_to_the_next_page_with_a_wipe_and_tells_the_app() {
    let image = pages_image(0);
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();

    let result = host.touch(Touch::Swipe(SwipeDirection::Left), START_TIME, &mut system);

    assert_eq!(result, EventResult::Handled);
    assert_eq!(host.widget_bounds(FIRST_LABEL), None);
    assert!(host.widget_bounds(LIST).is_some());
    assert_eq!(host.next_transition(), Transition::Left);
    assert_eq!(host.state().get(0), Some(1));
    assert!(host.needs_drawing());

    host.touch(Touch::Swipe(SwipeDirection::Right), START_TIME, &mut system);
    assert_eq!(host.widget_text(FIRST_LABEL), Some("First"));
    assert_eq!(host.next_transition(), Transition::Right);
    assert_eq!(host.state().get(0), Some(0));
}

#[test]
fn a_swipe_past_the_first_or_last_page_goes_to_the_screen() {
    let image = pages_image(0);
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let before_the_first = host.touch(Touch::Swipe(SwipeDirection::Right), START_TIME, &mut system);

    let image = pages_image(2);
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let past_the_last = host.touch(Touch::Swipe(SwipeDirection::Left), START_TIME, &mut system);
    let up = host.touch(Touch::Swipe(SwipeDirection::Up), START_TIME, &mut system);

    assert_eq!(
        [before_the_first, past_the_last, up],
        [EventResult::NotHandled; 3]
    );
    assert_eq!(host.widget_text(LAST_LABEL), Some("Last"));
}

#[test]
fn a_page_out_of_range_shows_the_last_page() {
    let image = pages_image(9);
    let mut system = FakeSystem::default();
    let host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();

    assert_eq!(host.widget_text(LAST_LABEL), Some("Last"));
}

#[test]
fn a_drag_mostly_across_leaves_a_list_that_runs_down_alone_so_the_swipe_turns_the_page() {
    let image = pages_image(1);
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let rows_before = row_areas(&host);

    host.touch(Touch::Press { x: 200, y: 100 }, START_TIME, &mut system);
    host.touch(
        Touch::Drag {
            x: 60,
            y: 80,
            start_x: 200,
            start_y: 100,
        },
        START_TIME,
        &mut system,
    );
    assert_eq!(row_areas(&host), rows_before);
    let swipe = host.touch(Touch::Swipe(SwipeDirection::Left), START_TIME, &mut system);

    assert_eq!(swipe, EventResult::Handled);
    assert_eq!(host.widget_text(LAST_LABEL), Some("Last"));
}

#[test]
fn a_drag_down_still_scrolls_the_list_on_its_page() {
    let image = pages_image(1);
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let rows_before = row_areas(&host);

    host.touch(Touch::Press { x: 120, y: 200 }, START_TIME, &mut system);
    host.touch(
        Touch::Drag {
            x: 130,
            y: 100,
            start_x: 120,
            start_y: 200,
        },
        START_TIME,
        &mut system,
    );

    assert_ne!(row_areas(&host), rows_before);
}

#[test]
fn the_page_bar_marks_the_page_on_show() {
    let image = pages_image(1);
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();
    let bright = rgb565(DEFAULT_THEME.color(ThemeColor::Muted));
    let dim = rgb565(DEFAULT_THEME.color(ThemeColor::Surface));
    let bar_row = 239;

    assert_eq!(screen.pixel(40, bar_row), dim);
    assert_eq!(screen.pixel(120, bar_row), bright);
    assert_eq!(screen.pixel(200, bar_row), dim);
}

/// Two pages whose page on show follows the app's `page`, and a button on the first that jumps to the second.
fn bound_pages_image() -> Vec<u8> {
    use Instruction::*;

    let mut writer = ImageWriter::new("bound", "Bound", AppKind::App, 1);
    let page = writer.variable("page", ValueType::Int, false, 0);
    let main = writer.string("main");
    let jump = writer.string("Jump");
    let second = writer.string("Second");
    let go_to_second = writer.handler(code(|code| {
        code.emit(PushSmall(1)).emit(WriteState(page));
    }));
    let page_on_show = writer.expression(code(|code| {
        code.emit(ReadState(page));
    }));
    let constant = PropertySource::Constant;
    writer.widget(
        WidgetKind::Column,
        1,
        &[(Property::ScreenName, constant(main as i32))],
    );
    writer.widget(
        WidgetKind::Pages,
        2,
        &[
            (Property::Grow, constant(1)),
            (Property::Value, PropertySource::Expression(page_on_show)),
        ],
    );
    writer.widget(WidgetKind::Column, 1, &[(Property::Grow, constant(1))]);
    writer.widget(
        WidgetKind::Button,
        0,
        &[
            (Property::Text, constant(jump as i32)),
            (Property::Height, constant(BUTTON_HEIGHT)),
            (Property::OnClick, PropertySource::Handler(go_to_second)),
        ],
    );
    writer.widget(WidgetKind::Column, 1, &[(Property::Grow, constant(1))]);
    writer.widget(WidgetKind::Label, 0, &[(Property::Text, constant(second as i32))]);
    writer.build()
}

#[test]
fn a_page_the_app_turns_to_wipes_in_the_way_a_swipe_would() {
    let image = bound_pages_image();
    let mut system = FakeSystem::default();
    let mut host = AppHost::open(&image, DEFAULT_THEME, START_TIME, &mut system).unwrap();
    let mut screen = FakeScreen::new();
    host.draw(&mut screen).unwrap();

    click_on(&mut host, 3, START_TIME, &mut system);

    assert_eq!(host.widget_text(5), Some("Second"));
    assert_eq!(host.widget_bounds(3), None);
    assert_eq!(host.next_transition(), Transition::Left);
    assert!(host.needs_drawing());
}
