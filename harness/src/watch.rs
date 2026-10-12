//! The test watch: one app open on the real app host, driven by touches, the button and the clock.

use std::collections::HashMap;
use std::time::Duration;

use neotime_app_host::{AppFailure, AppHost, EventResult, PushedEvent, Rect, Touch, SCREEN_AREA};
use neotime_runtime::events::{ButtonPress, SwipeDirection};
use neotime_runtime::{
    AppImage, EffectKind, Property, PropertySource, SystemValue, SystemValues, Value, WidgetKind, WidgetTree,
    DEFAULT_THEME,
};

use crate::fake_system::{FakeSystem, InstalledApp, RecordedEffect};
use crate::framebuffer::Framebuffer;
use crate::sources::{compile_package, compile_source_text, shared_library, OpenFailure, PackageKind};

// ============================================================================
// Limits
// ============================================================================

/// More wakeups than this in one `advance` means the app or the host asks to wake in a loop.
pub const MOST_WAKEUPS_PER_ADVANCE: usize = 100_000;

/// How far a swipe's finger travels, as the touch panel would report it.
const SWIPE_DISTANCE_IN_PIXELS: i32 = 80;

/// How many drag reports a drag or swipe sends between press and release.
const DRAG_STEPS: i32 = 4;

/// The middle of the screen, where swipes start.
const SCREEN_MIDDLE: i32 = 120;

// ============================================================================
// Opening
// ============================================================================

/// An app open on a fake watch, as a test sees and drives it.
pub struct TestWatch {
    host: AppHost<'static>,
    widgets: WidgetTree<'static>,
    system: FakeSystem,
    framebuffer: Framebuffer,
    is_screen_on: bool,
}

impl TestWatch {
    /// Compiles `apps/<id>/app.tsx` and opens it.
    pub fn open_app(id: &str) -> Result<TestWatch, OpenFailure> {
        TestWatch::open_package(PackageKind::App, id)
    }

    /// Compiles `faces/<id>/face.tsx` and opens it.
    pub fn open_face(id: &str) -> Result<TestWatch, OpenFailure> {
        TestWatch::open_package(PackageKind::Face, id)
    }

    pub fn open_package(kind: PackageKind, id: &str) -> Result<TestWatch, OpenFailure> {
        TestWatch::open_image(compile_package(kind, id)?)
    }

    /// Compiles app source text written in the test itself and opens it.
    pub fn open_source(source: &str) -> Result<TestWatch, OpenFailure> {
        TestWatch::open_image(compile_source_text(source)?)
    }

    /// Opens an image that is already built, on a fake system with its starting values.
    pub fn open_image(image: Vec<u8>) -> Result<TestWatch, OpenFailure> {
        TestWatch::open_image_on(image, FakeSystem::new())
    }

    /// Opens an image on a fake system the test has set up, such as one with a low battery.
    pub fn open_image_on(image: Vec<u8>, mut system: FakeSystem) -> Result<TestWatch, OpenFailure> {
        // The host borrows its image for as long as it lives, so each test watch keeps its own for good.
        let image: &'static [u8] = Box::leak(image.into_boxed_slice());
        let library = shared_library()?;
        let now = system.now_in_milliseconds;
        let mut host =
            AppHost::open(image, DEFAULT_THEME, now, &mut system).map_err(OpenFailure::CannotOpen)?;
        host.set_library(library).map_err(OpenFailure::CannotOpen)?;
        let widgets = WidgetTree::new(*host.image());
        Ok(TestWatch {
            host,
            widgets,
            system,
            framebuffer: Framebuffer::new(),
            is_screen_on: true,
        })
    }

    /// Compiles the apps with these ids and installs them, so a launcher lists them.
    pub fn install_apps(&mut self, ids: &[&str]) -> Result<(), OpenFailure> {
        let mut images: Vec<&'static [u8]> = Vec::new();
        let mut installed_apps = Vec::new();
        for id in ids {
            let image: &'static [u8] = Box::leak(compile_package(PackageKind::App, id)?.into_boxed_slice());
            let parsed = AppImage::parse(image).map_err(|error| OpenFailure::CannotOpen(error.into()))?;
            let text = |string| parsed.string(string).unwrap_or_default().to_string();
            installed_apps.push(InstalledApp {
                name: text(parsed.name_string),
                id: text(parsed.id_string),
            });
            images.push(image);
        }
        self.system.installed_apps = installed_apps;
        self.host.set_installed_apps(Box::leak(images.into_boxed_slice()));
        let now = self.now();
        self.host
            .system_values_changed(SystemValue::InstalledAppCount.bit(), now, &mut self.system);
        Ok(())
    }

    // ------------------------------------------------------------------------
    // Reaching inside
    // ------------------------------------------------------------------------

    pub fn host(&self) -> &AppHost<'static> {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut AppHost<'static> {
        &mut self.host
    }

    pub fn system(&self) -> &FakeSystem {
        &self.system
    }

    /// The app's id, as `define_app` names it.
    pub fn id(&self) -> &str {
        let image = self.host.image();
        image.string(image.id_string).unwrap_or_default()
    }

    /// The app's name, as the launcher shows it.
    pub fn name(&self) -> &str {
        let image = self.host.image();
        image.string(image.name_string).unwrap_or_default()
    }

    pub fn now(&self) -> Value {
        self.system.now_in_milliseconds
    }

    pub fn is_screen_on(&self) -> bool {
        self.is_screen_on
    }
}

// ============================================================================
// Touches and the button
// ============================================================================

impl TestWatch {
    fn send(&mut self, touch: Touch) -> EventResult {
        let now = self.now();
        self.host.touch(touch, now, &mut self.system)
    }

    /// A tap as the touch panel reports one: the finger down, up, then the tap itself.
    pub fn tap(&mut self, x: i32, y: i32) -> EventResult {
        self.send(Touch::Press { x, y });
        self.send(Touch::Release { x, y });
        self.send(Touch::Click { x, y })
    }

    /// A finger held still until the panel reports a long press, then lifted.
    pub fn long_press(&mut self, x: i32, y: i32) -> EventResult {
        self.send(Touch::Press { x, y });
        let result = self.send(Touch::LongPress { x, y });
        self.send(Touch::Release { x, y });
        result
    }

    pub fn double_tap(&mut self, x: i32, y: i32) -> EventResult {
        self.send(Touch::Press { x, y });
        self.send(Touch::Release { x, y });
        self.send(Touch::DoubleClick { x, y })
    }

    /// A finger pressed at one point, moved in even steps to another, and lifted; gives the last drag's result.
    pub fn drag(&mut self, from: (i32, i32), to: (i32, i32)) -> EventResult {
        let (start_x, start_y) = from;
        let (end_x, end_y) = to;
        self.send(Touch::Press {
            x: start_x,
            y: start_y,
        });
        let mut result = EventResult::NotHandled;
        for step in 1..=DRAG_STEPS {
            let x = start_x + (end_x - start_x) * step / DRAG_STEPS;
            let y = start_y + (end_y - start_y) * step / DRAG_STEPS;
            result = self.send(Touch::Drag {
                x,
                y,
                start_x,
                start_y,
            });
        }
        self.send(Touch::Release { x: end_x, y: end_y });
        result
    }

    /// A swipe across the middle of the screen: the drags the panel reports, then the swipe itself.
    pub fn swipe(&mut self, direction: SwipeDirection) -> EventResult {
        let (step_x, step_y) = match direction {
            SwipeDirection::Up => (0, -1),
            SwipeDirection::Down => (0, 1),
            SwipeDirection::Left => (-1, 0),
            SwipeDirection::Right => (1, 0),
        };
        let start_x = SCREEN_MIDDLE - step_x * SWIPE_DISTANCE_IN_PIXELS / 2;
        let start_y = SCREEN_MIDDLE - step_y * SWIPE_DISTANCE_IN_PIXELS / 2;
        let end_x = start_x + step_x * SWIPE_DISTANCE_IN_PIXELS;
        let end_y = start_y + step_y * SWIPE_DISTANCE_IN_PIXELS;
        self.drag((start_x, start_y), (end_x, end_y));
        self.send(Touch::Swipe(direction))
    }

    pub fn press_button(&mut self) -> EventResult {
        let now = self.now();
        self.host.press_button(ButtonPress::Press, now, &mut self.system)
    }

    pub fn long_press_button(&mut self) -> EventResult {
        let now = self.now();
        self.host
            .press_button(ButtonPress::LongPress, now, &mut self.system)
    }

    /// Taps the middle of a widget of the open screen, by its number in the image.
    pub fn tap_widget(&mut self, widget_index: u16) -> EventResult {
        match self.visible_area(widget_index) {
            Some(area) => self.tap(area.x + area.width / 2, area.y + area.height / 2),
            None => EventResult::NotHandled,
        }
    }

    /// Taps a widget at evenly spread points, `points_across` by `points_across`, such as every key of a keypad.
    pub fn tap_all_over(&mut self, widget_index: u16, points_across: i32) {
        let Some(area) = self.visible_area(widget_index) else {
            return;
        };
        for row in 0..points_across {
            for column in 0..points_across {
                let x = area.x + area.width * (2 * column + 1) / (2 * points_across);
                let y = area.y + area.height * (2 * row + 1) / (2 * points_across);
                self.tap(x, y);
            }
        }
    }

    /// Taps the shown widget whose text is exactly `text`, and fails listing what is shown if none is.
    #[track_caller]
    pub fn tap_text(&mut self, text: &str) -> EventResult {
        let found = self
            .visible_widgets()
            .into_iter()
            .find(|&widget_index| self.host.widget_text(widget_index) == Some(text));
        match found {
            Some(widget_index) => self.tap_widget(widget_index),
            None => panic!(
                "{}: no shown widget reads {text:?}; the screen shows {:?}",
                self.id(),
                self.visible_texts()
            ),
        }
    }
}

// ============================================================================
// Time, events and the screen
// ============================================================================

impl TestWatch {
    fn set_clock(&mut self, now: Value) {
        self.system.now_in_milliseconds = now;
    }

    /// Moves the clock on, waking the app at each time it asked for on the way, in order.
    #[track_caller]
    pub fn advance(&mut self, duration: Duration) {
        let target = self.now() + duration.as_millis() as Value;
        let mut wakeups = 0;
        while let Some(wakeup) = self.host.next_wakeup(self.now()) {
            // A wakeup due now is taken a millisecond later, so the clock always moves forward.
            let next = wakeup.max(self.now() + 1);
            if next > target {
                break;
            }
            wakeups += 1;
            assert!(
                wakeups <= MOST_WAKEUPS_PER_ADVANCE,
                "{}: woke more than {MOST_WAKEUPS_PER_ADVANCE} times in {duration:?}",
                self.id()
            );
            self.set_clock(next);
            self.host.tick(next, &mut self.system);
        }
        self.set_clock(target);
        self.host.tick(target, &mut self.system);
    }

    /// Hands the app an event from the phone or a sensor, and updates the system values it carries.
    pub fn deliver(&mut self, event: PushedEvent) -> EventResult {
        let now = self.now();
        match event {
            PushedEvent::HeartRate { beats_per_minute, .. } => {
                self.system.set(SystemValue::HeartRate, beats_per_minute);
                self.system.set(SystemValue::HeartRateMeasuredAt, now);
            }
            PushedEvent::Steps { steps_today } | PushedEvent::StepGoal { steps_today } => {
                self.system.set(SystemValue::StepsToday, steps_today);
            }
            PushedEvent::Battery { percent, .. } => self.system.set(SystemValue::BatteryPercent, percent),
            PushedEvent::Phone { is_connected } => self
                .system
                .set(SystemValue::IsPhoneConnected, is_connected as Value),
            PushedEvent::Notification { .. } => {
                let unread = self.system.read(SystemValue::UnreadNotifications);
                self.system.set(SystemValue::UnreadNotifications, unread + 1);
            }
            _ => {}
        }
        self.host.deliver(event, now, &mut self.system)
    }

    /// Changes a system value, such as the battery, and lets the app redraw what shows it.
    pub fn set_system_value(&mut self, value: SystemValue, number: Value) {
        self.system.set(value, number);
        let now = self.now();
        self.host
            .system_values_changed(value.bit(), now, &mut self.system);
    }

    pub fn screen_on(&mut self) -> EventResult {
        self.is_screen_on = true;
        let now = self.now();
        self.host.screen_turned_on(now, &mut self.system)
    }

    pub fn screen_off(&mut self) -> EventResult {
        self.is_screen_on = false;
        let now = self.now();
        self.host.screen_turned_off(now, &mut self.system)
    }
}

// ============================================================================
// Looking at the screen
// ============================================================================

impl TestWatch {
    /// The widgets of the open screen, by number, with each one's parent.
    fn screen_widgets(&self) -> Vec<(u16, Option<u16>)> {
        let mut found = Vec::new();
        let mut waiting = vec![(self.host.screen(), None)];
        while let Some((widget_index, parent)) = waiting.pop() {
            found.push((widget_index, parent));
            for child in self.widgets.children(widget_index) {
                waiting.push((child, Some(widget_index)));
            }
        }
        found.sort();
        found
    }

    /// Where a widget can be seen: shown, and inside the screen and every widget around it.
    pub fn visible_area(&self, widget_index: u16) -> Option<Rect> {
        let parents: HashMap<u16, Option<u16>> = self.screen_widgets().into_iter().collect();
        let (bounds, is_shown) = self.host.widget_bounds(widget_index)?;
        if !is_shown {
            return None;
        }
        let mut area = bounds.intersect(SCREEN_AREA)?;
        let mut parent = parents.get(&widget_index).copied().flatten();
        while let Some(parent_index) = parent {
            let (parent_bounds, _) = self.host.widget_bounds(parent_index)?;
            area = area.intersect(parent_bounds)?;
            parent = parents.get(&parent_index).copied().flatten();
        }
        (area.width > 0 && area.height > 0).then_some(area)
    }

    /// The open screen's widgets that can be seen, in the image's order.
    pub fn visible_widgets(&self) -> Vec<u16> {
        self.screen_widgets()
            .into_iter()
            .map(|(widget_index, _)| widget_index)
            .filter(|&widget_index| self.visible_area(widget_index).is_some())
            .collect()
    }

    /// The seen widgets that answer a tap: a click, press or release handler, or an input's `onChange`.
    pub fn tappable_widgets(&self) -> Vec<u16> {
        self.visible_widgets()
            .into_iter()
            .filter(|&widget_index| self.answers_taps(widget_index))
            .collect()
    }

    pub fn widget_kind(&self, widget_index: u16) -> Option<WidgetKind> {
        self.widgets.widget(widget_index).map(|widget| widget.kind)
    }

    fn answers_taps(&self, widget_index: u16) -> bool {
        let Some(widget) = self.widgets.widget(widget_index) else {
            return false;
        };
        let has_handler = |property| matches!(widget.property(property), Some(PropertySource::Handler(_)));
        let sends_change = matches!(
            widget.kind,
            WidgetKind::Switch
                | WidgetKind::Checkbox
                | WidgetKind::Radio
                | WidgetKind::Roller
                | WidgetKind::Slider
        );
        has_handler(Property::OnClick)
            || has_handler(Property::OnPress)
            || has_handler(Property::OnRelease)
            || (sends_change && has_handler(Property::OnChange))
            || widget.kind == WidgetKind::Dropdown
    }

    /// The text of every seen widget that shows some, in the image's order.
    pub fn visible_texts(&self) -> Vec<String> {
        self.visible_widgets()
            .into_iter()
            .filter_map(|widget_index| self.host.widget_text(widget_index))
            .filter(|text| !text.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Whether some seen widget's text contains `text`.
    pub fn shows_text(&self, text: &str) -> bool {
        self.visible_texts().iter().any(|shown| shown.contains(text))
    }

    pub fn screen_name(&self) -> Option<&str> {
        self.host.screen_name()
    }

    pub fn effects(&self) -> &[RecordedEffect] {
        self.system.effects()
    }

    /// The effects so far, leaving none, so a test can check what one step asked for.
    pub fn take_effects(&mut self) -> Vec<RecordedEffect> {
        self.system.take_effects()
    }

    /// The effects of one kind so far, such as every vibration.
    pub fn effects_of(&self, kind: EffectKind) -> Vec<&RecordedEffect> {
        self.effects()
            .iter()
            .filter(|effect| effect.kind == kind)
            .collect()
    }

    pub fn failures(&self) -> &[AppFailure] {
        self.system.failures()
    }

    /// Draws what changed since the last frame, as the watch would, and gives the whole screen.
    pub fn frame(&mut self) -> &Framebuffer {
        let Ok(()) = self.host.draw(&mut self.framebuffer);
        &self.framebuffer
    }

    /// Draws the whole screen from nothing, to compare with the frame built up from changes.
    pub fn fresh_frame(&mut self) -> Framebuffer {
        let mut framebuffer = Framebuffer::new();
        let theme = *self.host.theme();
        self.host.set_theme(theme);
        let Ok(()) = self.host.draw(&mut framebuffer);
        framebuffer
    }

    /// How many pixels the frame built up from changes differs from drawing the screen afresh.
    pub fn pixels_left_stale(&mut self) -> usize {
        self.frame();
        let built_up = self.framebuffer.clone();
        let fresh = self.fresh_frame();
        self.framebuffer = fresh.clone();
        built_up.differing_pixels(&fresh)
    }
}
