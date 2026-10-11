//! The loop that runs one open app: it builds the screen, sends events to handlers, carries out
//! their effects and redraws exactly the widgets whose values changed.

use alloc::vec::Vec;

use embedded_graphics::{pixelcolor::Rgb565, prelude::DrawTarget};
use neotime_runtime::{
    events::{ButtonPress, EventKind},
    verify,
    widgets::{Property, PropertyValue as ValueKind, DEEPEST_NESTING, MOST_WIDGETS},
    AppImage, AppState, Bindings, EffectKind, ImageError, Inputs, Interpreter, PropertySource, SystemValues,
    Theme, ThemeColor, Value, WidgetKind, WidgetTree,
};
use neotime_ui::ScreenRenderer;
use rlvgl_core::{
    renderer::{ClipRenderer, Renderer},
    widget::{Color, Rect},
};

use crate::layout::lay_out;
use crate::nodes::{empty_widget, to_color, Node, PropertyValue, Settings};
use crate::system::{button_value, swipe_value, AppEffect, AppFailure, Clocked, EventResult, System, Touch};

// ============================================================================
// Limits
// ============================================================================

/// The screen the host draws on.
pub const SCREEN_AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 240,
    height: 240,
};

/// How many screens `back()` can return through.
pub const SCREEN_HISTORY_DEPTH: usize = 8;

/// How many expressions an app may bind, one bit each in a mask.
const MOST_BOUND_EXPRESSIONS: usize = 64;

/// Why an app could not be opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenError {
    Image(ImageError),
    NoScreens,
}

impl From<ImageError> for OpenError {
    fn from(error: ImageError) -> Self {
        OpenError::Image(error)
    }
}

/// A screen change a handler asked for, carried out once its other effects are done.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Navigation {
    Go(u16),
    Back,
}

// ============================================================================
// The host
// ============================================================================

/// One open app and the widgets of its screen.
pub struct AppHost<'image> {
    image: AppImage<'image>,
    tree: WidgetTree<'image>,
    state: AppState,
    interpreter: Interpreter,
    bindings: Bindings,
    theme: Theme,
    screen_root: u16,
    screen_history: [u16; SCREEN_HISTORY_DEPTH],
    screen_history_length: usize,
    /// One bit for each expression the open screen's widgets read.
    screen_expressions: u64,
    nodes: Vec<Node>,
    /// One bit for each node to draw again.
    dirty_nodes: u128,
    whole_screen_dirty: bool,
    last_clock_update: Value,
    persistent_variables: u64,
    changed_persistent_variables: u64,
}

impl<'image> AppHost<'image> {
    /// Checks an image, starts its state from the defaults and opens its first screen.
    pub fn open(
        image_bytes: &'image [u8],
        theme: Theme,
        now: Value,
        system: &mut impl System,
    ) -> Result<Self, OpenError> {
        Self::open_with_saved_state(image_bytes, theme, now, system, |_, _| {})
    }

    /// Like `open`, with `load_saved` putting saved values over the defaults before anything is drawn.
    pub fn open_with_saved_state(
        image_bytes: &'image [u8],
        theme: Theme,
        now: Value,
        system: &mut impl System,
        load_saved: impl FnOnce(&AppImage, &mut AppState),
    ) -> Result<Self, OpenError> {
        let image = AppImage::parse(image_bytes)?;
        verify(&image)?;
        let mut state = AppState::new(&image)?;
        load_saved(&image, &mut state);
        let bindings = Bindings::load(&image)?;
        let tree = WidgetTree::new(image);
        let first_screen = tree.screens().next().ok_or(OpenError::NoScreens)?;

        let mut persistent_variables = 0;
        for variable in 0..image.state_count().min(MOST_BOUND_EXPRESSIONS as u16) {
            if image
                .state_variable(variable)
                .is_some_and(|variable| variable.persists)
            {
                persistent_variables |= 1 << variable;
            }
        }

        let mut host = AppHost {
            image,
            tree,
            state,
            interpreter: Interpreter::new(),
            bindings,
            theme,
            screen_root: first_screen,
            screen_history: [0; SCREEN_HISTORY_DEPTH],
            screen_history_length: 0,
            screen_expressions: 0,
            nodes: Vec::new(),
            dirty_nodes: 0,
            whole_screen_dirty: true,
            last_clock_update: now,
            persistent_variables,
            changed_persistent_variables: 0,
        };
        host.show_screen(first_screen, now, system);
        host.run_screen_event(Property::OnShow, &[], now, system, false);
        Ok(host)
    }

    pub fn image(&self) -> &AppImage<'image> {
        &self.image
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// The widget number of the open screen's root.
    pub fn screen(&self) -> u16 {
        self.screen_root
    }

    /// The open screen's name, if it has one.
    pub fn screen_name(&self) -> Option<&'image str> {
        let root = self.tree.widget(self.screen_root)?;
        match root.property(Property::ScreenName)? {
            PropertySource::Constant(string) => self.image.string(u16::try_from(string).ok()?),
            _ => None,
        }
    }

    /// The persistent variables changed since the last call, one bit each, for the embedder to save.
    pub fn take_changed_persistent_variables(&mut self) -> u64 {
        core::mem::take(&mut self.changed_persistent_variables)
    }

    // ------------------------------------------------------------------------
    // Building a screen
    // ------------------------------------------------------------------------

    fn show_screen(&mut self, root: u16, now: Value, system: &mut impl System) {
        let end = self.tree.subtree_end(root).unwrap_or(root + 1);
        let count = (end - root) as usize;
        let mut nodes: Vec<Node> = Vec::with_capacity(count.min(MOST_WIDGETS));
        let mut open_parents = [0u8; DEEPEST_NESTING + 1];
        let mut depth = 0;

        for widget_index in root..end {
            let Some(widget) = self.tree.widget(widget_index) else {
                break;
            };
            let node_index = nodes.len();
            while depth > 0 && nodes[open_parents[depth - 1] as usize].subtree_end as usize <= node_index {
                depth -= 1;
            }
            let subtree_end = self.tree.subtree_end(widget_index).unwrap_or(end) - root;
            nodes.push(Node {
                widget_index,
                kind: widget.kind,
                subtree_end: subtree_end as u8,
                parent: (depth > 0).then(|| open_parents[depth - 1]),
                settings: Settings::default(),
                bounds: SCREEN_AREA,
                shown: false,
                drawn_bounds: None,
                widget: empty_widget(widget.kind),
            });
            if widget.child_count > 0 && depth < open_parents.len() {
                open_parents[depth] = node_index as u8;
                depth += 1;
            }
        }

        self.nodes = nodes;
        self.screen_root = root;
        self.screen_expressions = 0;
        self.last_clock_update = now;

        let mut first_failure = None;
        for node_index in 0..self.nodes.len() {
            let Some(widget) = self.tree.widget(self.nodes[node_index].widget_index) else {
                continue;
            };
            for entry in widget.properties().flatten() {
                match entry.source {
                    PropertySource::Constant(constant) => {
                        let value = match entry.property.value() {
                            ValueKind::Text => match self.image.string(constant as u16) {
                                Some(text) => PropertyValue::Text(text),
                                None => continue,
                            },
                            _ => PropertyValue::Number(constant as Value),
                        };
                        self.nodes[node_index].apply(entry.property, value);
                    }
                    PropertySource::Expression(expression) => {
                        if (expression as usize) < MOST_BOUND_EXPRESSIONS {
                            self.screen_expressions |= 1 << expression;
                        }
                        let failure = self.evaluate_into(node_index, entry.property, expression, now, system);
                        first_failure = first_failure.or(failure);
                    }
                    PropertySource::Handler(_) => {}
                }
            }
        }

        lay_out(&mut self.nodes, SCREEN_AREA);
        for node in &mut self.nodes {
            node.rebuild_widget(&self.theme);
        }
        self.whole_screen_dirty = true;
        self.dirty_nodes = 0;
        if let Some(failure) = first_failure {
            system.app_failed(failure);
        }
    }

    /// Works out one bound property and stores it; returns the failure if the expression failed.
    fn evaluate_into(
        &mut self,
        node_index: usize,
        property: Property,
        expression: u16,
        now: Value,
        system: &impl SystemValues,
    ) -> Option<AppFailure> {
        let clocked = Clocked { values: system, now };
        let result = self
            .interpreter
            .evaluate_expression(&self.image, expression, &self.state, &clocked);
        let failed = |failure| Some(AppFailure::Expression { expression, failure });

        let value = match result {
            Ok(value) => value,
            Err(failure) => return failed(failure),
        };
        let applied = match property.value() {
            ValueKind::Text => match self.interpreter.text(&self.image, value) {
                Ok(text) => self.nodes[node_index].apply(property, PropertyValue::Text(text)),
                Err(failure) => return failed(failure),
            },
            _ => self.nodes[node_index].apply(property, PropertyValue::Number(value)),
        };
        if applied {
            self.dirty_nodes |= 1 << node_index;
        }
        None
    }

    /// Works out again every property bound to one of `expressions`, then redoes the layout.
    fn refresh(&mut self, expressions: u64, now: Value, system: &mut impl System) {
        let expressions = expressions & self.screen_expressions;
        if expressions == 0 {
            return;
        }

        let mut first_failure = None;
        for node_index in 0..self.nodes.len() {
            let Some(widget) = self.tree.widget(self.nodes[node_index].widget_index) else {
                continue;
            };
            for entry in widget.properties().flatten() {
                let PropertySource::Expression(expression) = entry.source else {
                    continue;
                };
                if (expression as usize) < MOST_BOUND_EXPRESSIONS && expressions & (1 << expression) != 0 {
                    let failure = self.evaluate_into(node_index, entry.property, expression, now, system);
                    first_failure = first_failure.or(failure);
                }
            }
        }

        self.update_layout();
        if let Some(failure) = first_failure {
            system.app_failed(failure);
        }
    }

    /// Lays the screen out again, and marks every node that moved, appeared or vanished.
    fn update_layout(&mut self) {
        if self.dirty_nodes == 0 {
            return;
        }
        let before: Vec<(Rect, bool)> = self.nodes.iter().map(|node| (node.bounds, node.shown)).collect();
        lay_out(&mut self.nodes, SCREEN_AREA);
        for (node_index, node) in self.nodes.iter_mut().enumerate() {
            if before[node_index] != (node.bounds, node.shown) {
                self.dirty_nodes |= 1 << node_index;
            }
            if self.dirty_nodes & (1 << node_index) != 0 {
                node.rebuild_widget(&self.theme);
            }
        }
    }

    // ------------------------------------------------------------------------
    // Events
    // ------------------------------------------------------------------------

    /// Sends a touch to the topmost widget under it that answers it, or a swipe to the screen.
    pub fn touch(&mut self, touch: Touch, now: Value, system: &mut impl System) -> EventResult {
        let (property, x, y, fields): (Property, i32, i32, [Value; 4]) = match touch {
            Touch::Swipe(direction) => {
                return self.run_screen_event(Property::OnSwipe, &[swipe_value(direction)], now, system, true)
            }
            Touch::Click { x, y } => (Property::OnClick, x, y, [x as Value, y as Value, 0, 0]),
            Touch::LongPress { x, y } => (Property::OnLongPress, x, y, [x as Value, y as Value, 0, 0]),
            Touch::DoubleClick { x, y } => (Property::OnDoubleClick, x, y, [x as Value, y as Value, 0, 0]),
            Touch::Press { x, y } => (Property::OnPress, x, y, [x as Value, y as Value, 0, 0]),
            Touch::Release { x, y } => (Property::OnRelease, x, y, [x as Value, y as Value, 0, 0]),
            Touch::Drag {
                x,
                y,
                start_x,
                start_y,
            } => (
                Property::OnDrag,
                x,
                y,
                [x as Value, y as Value, start_x as Value, start_y as Value],
            ),
        };
        let field_count = EventKind::for_property(property).map_or(0, |event| event.fields().len());

        let Some((node_index, handler_property, handler)) = self.hit_test(property, x, y) else {
            return EventResult::NotHandled;
        };
        if handler_property == Property::OnChange {
            let value = self.changed_value(node_index, x);
            return self.run_handler(handler, EventKind::Change, &[value], now, system, true);
        }
        self.run_handler(
            handler,
            EventKind::for_property(property).unwrap_or(EventKind::Click),
            &fields[..field_count],
            now,
            system,
            true,
        )
    }

    /// Sends a side button press to the screen's `onButton`.
    pub fn press_button(&mut self, press: ButtonPress, now: Value, system: &mut impl System) -> EventResult {
        self.run_screen_event(Property::OnButton, &[button_value(press)], now, system, true)
    }

    /// The topmost shown widget under a point that answers the event, or a switch or slider's `onChange`.
    fn hit_test(&self, property: Property, x: i32, y: i32) -> Option<(usize, Property, u16)> {
        let is_inside = |bounds: Rect| {
            x >= bounds.x && x < bounds.x + bounds.width && y >= bounds.y && y < bounds.y + bounds.height
        };
        for (node_index, node) in self.nodes.iter().enumerate().rev() {
            if !node.shown || !is_inside(node.bounds) {
                continue;
            }
            let Some(widget) = self.tree.widget(node.widget_index) else {
                continue;
            };
            if let Some(PropertySource::Handler(handler)) = widget.property(property) {
                return Some((node_index, property, handler));
            }
            let sends_change = match node.kind {
                WidgetKind::Switch => property == Property::OnClick,
                WidgetKind::Slider => matches!(property, Property::OnClick | Property::OnDrag),
                _ => false,
            };
            if sends_change {
                if let Some(PropertySource::Handler(handler)) = widget.property(Property::OnChange) {
                    return Some((node_index, Property::OnChange, handler));
                }
            }
        }
        None
    }

    /// The value a tap gives a switch or slider: the switch flipped, or the slider's value under `x`.
    fn changed_value(&self, node_index: usize, x: i32) -> Value {
        let node = &self.nodes[node_index];
        let settings = &node.settings;
        match node.kind {
            WidgetKind::Switch => (settings.value == 0) as Value,
            _ => {
                let width = node.bounds.width.max(1) as Value;
                let along = (x - node.bounds.x).clamp(0, node.bounds.width) as Value;
                let span = settings.maximum as Value - settings.minimum as Value;
                settings.minimum as Value + span * along / width
            }
        }
    }

    /// Runs a handler set on the screen's root, such as `onSwipe`.
    fn run_screen_event(
        &mut self,
        property: Property,
        fields: &[Value],
        now: Value,
        system: &mut impl System,
        may_navigate: bool,
    ) -> EventResult {
        let handler = match self
            .tree
            .widget(self.screen_root)
            .and_then(|root| root.property(property))
        {
            Some(PropertySource::Handler(handler)) => handler,
            _ => return EventResult::NotHandled,
        };
        let event = EventKind::for_property(property).unwrap_or(EventKind::Show);
        self.run_handler(handler, event, fields, now, system, may_navigate)
    }

    /// Runs a handler, carries out its effects and redraws what its changes affect.
    fn run_handler(
        &mut self,
        handler: u16,
        event: EventKind,
        fields: &[Value],
        now: Value,
        system: &mut impl System,
        may_navigate: bool,
    ) -> EventResult {
        let clocked = Clocked {
            values: &*system,
            now,
        };
        let outcome =
            match self
                .interpreter
                .run_handler(&self.image, handler, &mut self.state, fields, &clocked)
            {
                Ok(outcome) => outcome,
                Err(failure) => {
                    system.app_failed(AppFailure::Handler {
                        handler,
                        event,
                        failure,
                    });
                    return EventResult::Failed;
                }
            };

        let changed_variables = outcome.changed_variables();
        self.changed_persistent_variables |= changed_variables & self.persistent_variables;

        let mut navigation = None;
        for effect in outcome.effects() {
            let app_effect = AppEffect {
                kind: effect.kind,
                arguments: effect.arguments,
                image: &self.image,
                interpreter: &self.interpreter,
            };
            let wanted = match effect.kind {
                EffectKind::Go if may_navigate => app_effect
                    .text(0)
                    .and_then(|name| self.tree.screen_named(name))
                    .map(Navigation::Go),
                EffectKind::Back if may_navigate && self.screen_history_length > 0 => Some(Navigation::Back),
                _ => None,
            };
            match wanted {
                Some(wanted) => navigation = Some(wanted),
                None => system.carry_out(&app_effect),
            }
        }

        let affected = self.bindings.affected_by(Inputs {
            state_slots: changed_variables,
            system_values: 0,
        });
        let affected_mask = affected
            .filter(|&expression| expression < MOST_BOUND_EXPRESSIONS)
            .fold(0u64, |mask, expression| mask | 1 << expression);
        self.refresh(affected_mask, now, system);

        if let Some(navigation) = navigation {
            self.navigate(navigation, now, system);
        }
        EventResult::Handled
    }

    fn navigate(&mut self, navigation: Navigation, now: Value, system: &mut impl System) {
        let next_screen = match navigation {
            Navigation::Go(root) if root == self.screen_root => return,
            Navigation::Go(root) => {
                if self.screen_history_length == SCREEN_HISTORY_DEPTH {
                    self.screen_history.copy_within(1.., 0);
                    self.screen_history_length -= 1;
                }
                self.screen_history[self.screen_history_length] = self.screen_root;
                self.screen_history_length += 1;
                root
            }
            Navigation::Back => {
                self.screen_history_length -= 1;
                self.screen_history[self.screen_history_length]
            }
        };

        self.run_screen_event(Property::OnHide, &[], now, system, false);
        self.show_screen(next_screen, now, system);
        self.run_screen_event(Property::OnShow, &[], now, system, false);
    }

    // ------------------------------------------------------------------------
    // Time, system values and theme
    // ------------------------------------------------------------------------

    /// Redraws whatever the clock has changed on the open screen since the last tick.
    pub fn tick(&mut self, now: Value, system: &mut impl System) {
        let due = self
            .bindings
            .due_between_among(self.last_clock_update, now, self.screen_expressions)
            .filter(|&expression| expression < MOST_BOUND_EXPRESSIONS)
            .fold(0u64, |mask, expression| mask | 1 << expression);
        self.last_clock_update = now;
        self.refresh(due, now, system);
    }

    /// When the clock next changes something on the open screen, or `None` if nothing on it reads it.
    pub fn next_wakeup(&self, now: Value) -> Option<Value> {
        self.bindings
            .next_redraw_after_among(now, self.screen_expressions)
    }

    /// Redraws what reads the system values that changed, given as `SystemValue::bit` values or'd together.
    pub fn system_values_changed(&mut self, changed: u32, now: Value, system: &mut impl System) {
        let affected = self
            .bindings
            .affected_by(Inputs {
                state_slots: 0,
                system_values: changed,
            })
            .filter(|&expression| expression < MOST_BOUND_EXPRESSIONS)
            .fold(0u64, |mask, expression| mask | 1 << expression);
        self.refresh(affected, now, system);
    }

    /// Changes the theme and redraws every colour, without running any app code.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        for node in &mut self.nodes {
            node.rebuild_widget(&self.theme);
        }
        self.whole_screen_dirty = true;
    }

    // ------------------------------------------------------------------------
    // Drawing
    // ------------------------------------------------------------------------

    /// Whether `draw` has anything to do.
    pub fn needs_drawing(&self) -> bool {
        self.whole_screen_dirty || self.dirty_nodes != 0
    }

    /// The image's numbers of the widgets waiting to be drawn again.
    pub fn dirty_widgets(&self) -> impl Iterator<Item = u16> + '_ {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(node_index, _)| self.whole_screen_dirty || self.dirty_nodes & (1 << node_index) != 0)
            .map(|(_, node)| node.widget_index)
    }

    /// Where layout put a widget of the open screen, and whether it is shown.
    pub fn widget_bounds(&self, widget_index: u16) -> Option<(Rect, bool)> {
        let node = self.node(widget_index)?;
        Some((node.bounds, node.shown))
    }

    /// The text a widget of the open screen shows.
    pub fn widget_text(&self, widget_index: u16) -> Option<&str> {
        Some(self.node(widget_index)?.text())
    }

    fn node(&self, widget_index: u16) -> Option<&Node> {
        self.nodes.iter().find(|node| node.widget_index == widget_index)
    }

    /// Draws every widget that changed since the last call, and only those parts of the screen.
    pub fn draw<Screen>(&mut self, screen: &mut Screen) -> Result<(), Screen::Error>
    where
        Screen: DrawTarget<Color = Rgb565>,
    {
        let regions = self.dirty_regions();
        for region in &regions {
            self.paint(screen, *region)?;
        }

        for (node_index, node) in self.nodes.iter_mut().enumerate() {
            if self.whole_screen_dirty || self.dirty_nodes & (1 << node_index) != 0 {
                node.drawn_bounds = node.shown.then_some(node.bounds);
            }
        }
        self.whole_screen_dirty = false;
        self.dirty_nodes = 0;
        Ok(())
    }

    /// The parts of the screen to paint again: where each changed widget was and where it is now.
    fn dirty_regions(&self) -> Vec<Rect> {
        let mut regions: Vec<Rect> = Vec::new();
        if self.whole_screen_dirty {
            regions.push(SCREEN_AREA);
            return regions;
        }
        for (node_index, node) in self.nodes.iter().enumerate() {
            if self.dirty_nodes & (1 << node_index) == 0 {
                continue;
            }
            let now_shown = node.shown.then_some(node.bounds);
            for region in [node.drawn_bounds, now_shown].into_iter().flatten() {
                add_region(&mut regions, region);
            }
        }
        regions
    }

    /// Paints one region from the background up, every shown widget clipped to it.
    fn paint<Screen>(&self, screen: &mut Screen, region: Rect) -> Result<(), Screen::Error>
    where
        Screen: DrawTarget<Color = Rgb565>,
    {
        let Some(region) = region.intersect(SCREEN_AREA) else {
            return Ok(());
        };
        let background = to_color(self.theme.color(ThemeColor::Background));
        let mut renderer = ScreenRenderer::new(screen).with_background_color(background);
        renderer.fill_rect(region, background);
        renderer.finish()?;

        for (node_index, node) in self.nodes.iter().enumerate() {
            if !node.shown || node.bounds.intersect(region).is_none() {
                continue;
            }
            let backdrop = self.backdrop(node_index, background);
            let mut renderer = ScreenRenderer::new(screen).with_background_color(backdrop);
            {
                let mut clipped = ClipRenderer::new(&mut renderer, region);
                node.widget.draw(&mut clipped);
            }
            renderer.finish()?;
        }
        Ok(())
    }

    /// The colour behind a node's content, for mixing the soft edges of its text and shapes.
    fn backdrop(&self, node_index: usize, screen_background: Color) -> Color {
        let mut next = Some(node_index);
        while let Some(index) = next {
            let node = &self.nodes[index];
            if let Some(color) = node.own_background(&self.theme) {
                return color;
            }
            next = node.parent.map(|parent| parent as usize);
        }
        screen_background
    }
}

/// Adds a region to paint, merging it with any it overlaps.
fn add_region(regions: &mut Vec<Rect>, region: Rect) {
    if region.width <= 0 || region.height <= 0 {
        return;
    }
    let mut merged = region;
    while let Some(position) = regions
        .iter()
        .position(|existing| existing.intersect(merged).is_some())
    {
        merged = merged.union(regions.swap_remove(position));
    }
    regions.push(merged);
}
