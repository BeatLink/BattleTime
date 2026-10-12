//! The loop that runs one app: it builds the screen, sends events and subscriptions to handlers,
//! carries out their effects, saves state and redraws exactly the widgets whose values changed.
//!
//! An app is open, with a screen, or closed, with only its state, so subscriptions can run in the
//! background without any widgets.

use alloc::{boxed::Box, string::String, vec::Vec};

use neotime_runtime::{
    events::{ButtonPress, EventKind, SwipeDirection},
    state::SaveBufferFull,
    subscriptions::{Subscription, SubscriptionKind},
    verify,
    widgets::{Property, PropertyValue as ValueKind, MOST_WIDGETS},
    AppImage, AppKind, AppState, Bindings, EffectKind, Failure, ImageError, Inputs, Interpreter, PropertySource,
    SystemValue, SystemValues, Theme, Value, WidgetKind, WidgetTree, NONE,
};
use rlvgl_core::widget::Rect;

use crate::drawing::pickers::{is_picker, picker_tapped};
use crate::drawing::{
    app_list_rows, app_row_at, cell_at, dropdown_option_at, label_scrolls, matrix_buttons, measure_node,
    option_row_height, roller_rows, text_file_lines_in_view, text_file_rows, DrawContext,
    LABEL_SCROLL_STEP_IN_MILLISECONDS,
};
use crate::fonts::line_height;
use crate::layout::{clips_children, content_height, lay_out};
use crate::nodes::{row_slots, slots_in_view, Extra, Node, PropertyValue};

#[path = "painting.rs"]
mod painting;

use crate::subscriptions::{
    derive_inputs_in, load_subscriptions, PushedEvent, SubscriptionMemory, FAILURES_BEFORE_TURNING_OFF,
};
use crate::system::{button_value, swipe_value, AppEffect, AppFailure, Clocked, EventResult, System, Touch};
use crate::text::WrappedLines;
use crate::text_file::{covers, font_from_file, LineIndex};
use painting::{ScrollMove, TransitionRun};
pub use painting::{Transition, STRIP_PIXELS};

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
const MOST_BOUND_EXPRESSIONS: usize = neotime_runtime::bindings::MOST_EXPRESSIONS;
/// Persistent variables are tracked one bit each in a `u64`.
const MOST_TRACKED_VARIABLES: u16 = 64;

/// The most bytes `save_changed_state` can need: every byte of state, plus each variable's key and header.
pub const LARGEST_SAVED_STATE_BYTES: usize =
    6 + neotime_runtime::image::MOST_STATE_BYTES + 64 * (1 + 255 + 4);

/// A widget faded out because it is disabled is drawn at this opacity.
const DISABLED_OPACITY: u8 = 128;

/// Why an app could not be opened, or a library not used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenError {
    Image(ImageError),
    NoScreens,
    NotALibrary,
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

/// A drag that is scrolling a Scroll or List: which node, and its offset when the drag began.
#[derive(Clone, Copy)]
struct ScrollDrag {
    node_index: usize,
    start_offset: i32,
}

/// Which widget, and which row of a repeated list, a node stands for.
#[derive(Clone, Copy, PartialEq, Eq)]
struct NodeKey {
    widget_index: u16,
    row: u16,
}

impl NodeKey {
    fn of(node: &Node) -> NodeKey {
        NodeKey {
            widget_index: node.widget_index,
            row: node.row,
        }
    }
}

fn mask_of(expressions: impl Iterator<Item = usize>) -> u128 {
    expressions
        .filter(|&expression| expression < MOST_BOUND_EXPRESSIONS)
        .fold(0u128, |mask, expression| mask | 1 << expression)
}

// ============================================================================
// The host
// ============================================================================

/// One app, open with the widgets of its screen, or closed with only its state.
pub struct AppHost<'image> {
    image: AppImage<'image>,
    tree: WidgetTree<'image>,
    library: Option<AppImage<'image>>,
    installed_app_icons: &'image [&'image [u8]],
    // The three big parts live on the heap, so opening an app never copies kilobytes across the stack.
    state: Box<AppState>,
    interpreter: Box<Interpreter>,
    bindings: Box<Bindings>,
    theme: Theme,
    is_open: bool,
    screen_root: u16,
    screen_history: [u16; SCREEN_HISTORY_DEPTH],
    screen_history_length: usize,
    /// One bit for each expression the open screen's widgets read.
    screen_expressions: u128,
    /// Each node is its own allocation, so a screen of many widgets never needs one large free block.
    #[allow(clippy::vec_box)]
    nodes: Vec<Box<Node>>,
    /// One bit for each node to draw again.
    dirty_nodes: u128,
    /// Where nodes that are gone were last drawn, to paint over.
    cleared_regions: Vec<Rect>,
    /// A widget was shown or hidden, so the nodes must be built again to match.
    nodes_out_of_date: bool,
    /// Whether the touch under way has scrolled a list, so its slide is not also a swipe.
    touch_scrolled: bool,
    scroll_move: Option<ScrollMove>,
    next_transition: Transition,
    transition_run: Option<TransitionRun>,
    whole_screen_dirty: bool,
    last_clock_update: Value,
    /// The time of the last call that gave one, for scrolling labels.
    now: Value,
    utc_offset: Value,
    persistent_variables: u64,
    changed_persistent_variables: u64,
    unsaved_persistent_variables: u64,
    subscriptions: Vec<SubscriptionMemory>,
    /// When timers were last checked, so each fires once for each time it comes due.
    last_timer_check: Value,
    latest_heart_rate: [Value; 2],
    latest_acceleration: [Value; 3],
    scroll_drag: Option<ScrollDrag>,
    /// Where each TextFile on the screen finds its lines in its file.
    text_files: Vec<(NodeKey, LineIndex)>,
}

impl<'image> AppHost<'image> {
    /// Checks an image, starts its state from the defaults and opens its first screen.
    pub fn open(
        image_bytes: &'image [u8],
        theme: Theme,
        now: Value,
        system: &mut impl System,
    ) -> Result<Self, OpenError> {
        Self::open_with_saved_state(image_bytes, theme, &[], now, system)
    }

    /// Like `open`, with the bytes `save_changed_state` once wrote put over the defaults first.
    ///
    /// Saved state that does not fit, such as from another app, is ignored.
    pub fn open_with_saved_state(
        image_bytes: &'image [u8],
        theme: Theme,
        saved_state: &[u8],
        now: Value,
        system: &mut impl System,
    ) -> Result<Self, OpenError> {
        let mut host = Self::load(image_bytes, theme, saved_state, now, system)?;
        host.bring_to_front(now, system);
        Ok(host)
    }

    /// Opens an app into `slot`, like `open_with_saved_state`, and returns the host there.
    ///
    /// The host is built in a step of its own that returns before the app's first handlers run, so
    /// the stack never holds its copies and those handlers at once.
    pub fn open_into<'slot>(
        slot: &'slot mut Option<Self>,
        image_bytes: &'image [u8],
        theme: Theme,
        saved_state: &[u8],
        now: Value,
        system: &mut impl System,
    ) -> Result<&'slot mut Self, OpenError> {
        Self::load_into(slot, image_bytes, theme, saved_state, now, system)?;
        let host = slot.as_mut().ok_or(OpenError::NoScreens)?;
        host.bring_to_front(now, system);
        Ok(host)
    }

    #[inline(never)]
    fn load_into(
        slot: &mut Option<Self>,
        image_bytes: &'image [u8],
        theme: Theme,
        saved_state: &[u8],
        now: Value,
        system: &mut impl System,
    ) -> Result<(), OpenError> {
        *slot = Some(Self::load(image_bytes, theme, saved_state, now, system)?);
        Ok(())
    }

    /// Loads an app's state without opening it, so its subscriptions can run in the background.
    pub fn open_in_background(
        image_bytes: &'image [u8],
        theme: Theme,
        saved_state: &[u8],
        now: Value,
        system: &mut impl System,
    ) -> Result<Self, OpenError> {
        Self::load(image_bytes, theme, saved_state, now, system)
    }

    fn load(
        image_bytes: &'image [u8],
        theme: Theme,
        saved_state: &[u8],
        now: Value,
        system: &mut impl System,
    ) -> Result<Self, OpenError> {
        let image = AppImage::parse(image_bytes)?;
        verify(&image)?;
        // Each big part is boxed empty and filled in place, so none is ever built on the stack.
        let mut state = Box::new(AppState::EMPTY);
        state.load_layout(&image)?;
        if !saved_state.is_empty() {
            state.load_persistent(&image, saved_state);
        }
        let mut bindings = Box::new(Bindings::EMPTY);
        bindings.load_into(&image)?;
        let tree = WidgetTree::new(image);
        let first_screen = tree.screens().next().ok_or(OpenError::NoScreens)?;

        let mut persistent_variables = 0;
        for variable in 0..image.state_count().min(MOST_TRACKED_VARIABLES) {
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
            library: None,
            installed_app_icons: &[],
            state,
            interpreter: Box::new(Interpreter::new()),
            bindings,
            theme,
            is_open: false,
            screen_root: first_screen,
            screen_history: [0; SCREEN_HISTORY_DEPTH],
            screen_history_length: 0,
            screen_expressions: 0,
            nodes: Vec::new(),
            dirty_nodes: 0,
            cleared_regions: Vec::new(),
            nodes_out_of_date: false,
            touch_scrolled: false,
            scroll_move: None,
            next_transition: Transition::None,
            transition_run: None,
            whole_screen_dirty: true,
            last_clock_update: now,
            now,
            utc_offset: system.read(SystemValue::UtcOffset),
            persistent_variables,
            changed_persistent_variables: 0,
            unsaved_persistent_variables: 0,
            subscriptions: load_subscriptions(&image),
            last_timer_check: now,
            latest_heart_rate: [0; 2],
            latest_acceleration: [0; 3],
            scroll_drag: None,
            text_files: Vec::new(),
        };
        host.run_derives(Inputs::NOTHING, true, now, system);
        Ok(host)
    }

    /// Gives the host each installed app's icon, as `AppImage::resource_bytes` gives it, for a launcher.
    pub fn set_installed_app_icons(&mut self, icons: &'image [&'image [u8]]) {
        self.installed_app_icons = icons;
        self.redraw_whole_screen();
    }

    /// Gives the host the shared library, whose images apps name from 32768 up.
    ///
    /// What reads the library's functions is worked out again, the derive subscriptions run, and the
    /// open screen is built again, since until now every call into the library failed quietly.
    pub fn set_library(
        &mut self,
        library_bytes: &'image [u8],
        now: Value,
        system: &mut impl System,
    ) -> Result<(), OpenError> {
        let library = AppImage::parse(library_bytes)?;
        verify(&library)?;
        if library.kind != AppKind::Library {
            return Err(OpenError::NotALibrary);
        }
        self.library = Some(library);
        self.bindings
            .load_into_with_library(&self.image, self.library.as_ref())?;
        for memory in self.subscriptions.iter_mut() {
            if memory.subscription.kind == SubscriptionKind::Derive {
                memory.derive_inputs = derive_inputs_in(&self.image, self.library.as_ref(), memory.subscription.handler);
            }
        }
        self.run_derives(Inputs::NOTHING, true, now, system);
        if self.is_open {
            let root = self.screen_root;
            self.show_screen(root, now, system);
        }
        Ok(())
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

    /// Whether the app is in front with a screen, rather than running only in the background.
    pub fn is_open(&self) -> bool {
        self.is_open
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

    // ------------------------------------------------------------------------
    // The app's life
    // ------------------------------------------------------------------------

    /// Brings the app to the front: runs its `Open` subscriptions and shows its first screen.
    pub fn bring_to_front(&mut self, now: Value, system: &mut impl System) {
        if self.is_open {
            return;
        }
        self.note_time(now, system);
        self.is_open = true;
        self.screen_history_length = 0;
        for memory in &mut self.subscriptions {
            memory.failures_in_a_row = 0;
        }
        self.run_subscriptions_of(SubscriptionKind::Open, None, &[], &[], now, system);
        let first_screen = self.tree.screens().next().unwrap_or(0);
        self.show_screen(first_screen, now, system);
        self.run_screen_event(Property::OnShow, &[], now, system, false);
    }

    /// Sends the app to the background: hides its screen, runs `Close` and frees its widgets.
    pub fn close(&mut self, now: Value, system: &mut impl System) {
        if !self.is_open {
            return;
        }
        self.note_time(now, system);
        self.run_screen_event(Property::OnHide, &[], now, system, false);
        self.run_subscriptions_of(SubscriptionKind::Close, None, &[], &[], now, system);
        self.is_open = false;
        self.nodes = Vec::new();
        self.screen_expressions = 0;
        self.dirty_nodes = 0;
        self.redraw_whole_screen();
        self.scroll_drag = None;
        self.scroll_move = None;
    }

    /// Tells an open app that the display turned on, through its `ScreenOn` subscriptions.
    pub fn screen_turned_on(&mut self, now: Value, system: &mut impl System) -> EventResult {
        self.note_time(now, system);
        match self.is_open {
            true => self.run_subscriptions_of(SubscriptionKind::ScreenOn, None, &[], &[], now, system),
            false => EventResult::NotHandled,
        }
    }

    /// Tells an open app that the display turned off, through its `ScreenOff` subscriptions.
    pub fn screen_turned_off(&mut self, now: Value, system: &mut impl System) -> EventResult {
        self.note_time(now, system);
        match self.is_open {
            true => self.run_subscriptions_of(SubscriptionKind::ScreenOff, None, &[], &[], now, system),
            false => EventResult::NotHandled,
        }
    }

    fn note_time(&mut self, now: Value, system: &impl SystemValues) {
        self.now = now;
        self.utc_offset = system.read(SystemValue::UtcOffset);
    }

    // ------------------------------------------------------------------------
    // Saving state
    // ------------------------------------------------------------------------

    /// The persistent variables changed since the last call, one bit each, for the embedder to save.
    pub fn take_changed_persistent_variables(&mut self) -> u64 {
        core::mem::take(&mut self.changed_persistent_variables)
    }

    /// Writes the saved state into `out` if a persistent variable changed since the last save.
    ///
    /// Returns how many bytes it wrote, or `None` when nothing needs saving. `out` of
    /// `LARGEST_SAVED_STATE_BYTES` always fits.
    pub fn save_changed_state(&mut self, out: &mut [u8]) -> Result<Option<usize>, SaveBufferFull> {
        if self.unsaved_persistent_variables == 0 {
            return Ok(None);
        }
        let length = self.state.save_persistent(&self.image, out)?;
        self.unsaved_persistent_variables = 0;
        Ok(Some(length))
    }

    // ------------------------------------------------------------------------
    // Building a screen
    // ------------------------------------------------------------------------

    fn show_screen(&mut self, root: u16, now: Value, system: &mut impl System) {
        self.nodes = Vec::new();
        self.cleared_regions.clear();
        self.screen_root = root;
        self.last_clock_update = now;
        self.scroll_drag = None;
        self.scroll_move = None;
        self.text_files = Vec::new();

        let mut failure = self.build_nodes(now, system);
        self.measure_and_lay_out();
        for _ in 0..2 {
            if !self.rows_in_view_moved() {
                break;
            }
            failure = failure.or(self.build_nodes(now, system));
            self.measure_and_lay_out();
        }
        self.fill_text_files(system);
        self.redraw_whole_screen();
        self.dirty_nodes = 0;
        if let Some(failure) = failure {
            system.app_failed(failure);
        }
    }

    /// Makes the nodes match what can show: one for each widget whose parents are all visible.
    ///
    /// A node that already exists is kept with its values, scroll and marks; a new one has its
    /// properties worked out. So a hidden part of the screen, such as a closed dialog, holds no UI
    /// heap, and showing it costs only its own nodes.
    fn build_nodes(&mut self, now: Value, system: &impl SystemValues) -> Option<AppFailure> {
        let was_dirty: Vec<bool> = (0..self.nodes.len())
            .map(|index| self.whole_screen_dirty || self.dirty_nodes & (1 << index) != 0)
            .collect();
        let drag = self
            .scroll_drag
            .and_then(|drag| Some((NodeKey::of(self.nodes.get(drag.node_index)?), drag)));
        let scrolled = self.scroll_move.take().and_then(|scroll_move| {
            Some((NodeKey::of(self.nodes.get(scroll_move.node_index)?), scroll_move))
        });
        let mut old: Vec<Option<Box<Node>>> =
            core::mem::take(&mut self.nodes).into_iter().map(Some).collect();
        self.dirty_nodes = 0;

        let mut first_failure = None;
        let mut created: Vec<usize> = Vec::new();
        let mut pending: Vec<(u16, u16, Option<u8>)> = alloc::vec![(self.screen_root, 0, None)];
        while let Some((widget_index, row, parent)) = pending.pop() {
            let Some(widget) = self.tree.widget(widget_index) else {
                continue;
            };
            if self.nodes.len() >= MOST_WIDGETS {
                break;
            }
            let node_index = self.nodes.len();
            let key = NodeKey { widget_index, row };
            let kept = old
                .iter()
                .position(|node| node.as_ref().is_some_and(|node| NodeKey::of(node) == key))
                .and_then(|place| Some((old[place].take()?, was_dirty[place])));
            match kept {
                Some((mut node, dirty)) => {
                    node.parent = parent;
                    self.nodes.push(node);
                    if dirty {
                        self.dirty_nodes |= 1 << node_index;
                    }
                }
                None => {
                    let mut node = Node::new(widget_index, widget.kind, 0, parent, SCREEN_AREA);
                    node.row = row;
                    self.nodes.push(Box::new(node));
                    first_failure = first_failure.or(self.fill_node(node_index, now, system));
                    self.dirty_nodes |= 1 << node_index;
                    created.push(node_index);
                }
            }
            if self.nodes[node_index].settings.visible {
                let children: Vec<u16> = self.tree.children(widget_index).collect();
                let rows = match widget.kind {
                    WidgetKind::Repeat => self.rows_to_build(node_index, now, system),
                    _ => alloc::vec![row],
                };
                for &child_row in rows.iter().rev() {
                    for &child in children.iter().rev() {
                        pending.push((child, child_row, Some(node_index as u8)));
                    }
                }
            }
        }

        let count = self.nodes.len();
        let mut ends: Vec<usize> = (1..=count).collect();
        for index in (0..count).rev() {
            if let Some(parent) = self.nodes[index].parent {
                ends[parent as usize] = ends[parent as usize].max(ends[index]);
            }
        }
        for (node, end) in self.nodes.iter_mut().zip(ends) {
            node.subtree_end = end as u8;
        }
        // During a scroll the panel moves a list's old pixels and draws the rows coming into view, so
        // rows built or dropped inside it need no drawing of their own.
        let scrolled_list = scrolled.as_ref().and_then(|(key, _)| {
            let node_index = self.nodes.iter().position(|node| NodeKey::of(node) == *key)?;
            Some((node_index, self.nodes[node_index].covered_area()?))
        });
        if let Some((list, _)) = scrolled_list {
            let inside = list + 1..self.nodes[list].subtree_end as usize;
            for &node_index in created.iter().filter(|index| inside.contains(index)) {
                self.dirty_nodes &= !(1 << node_index);
            }
        }
        for node in old.into_iter().flatten() {
            let Some(area) = node.drawn_bounds else {
                continue;
            };
            let moved_with_the_list =
                scrolled_list.is_some_and(|(_, list_area)| list_area.intersect(area) == Some(area));
            if !moved_with_the_list {
                self.cleared_regions.push(area);
            }
        }
        self.scroll_drag = drag.and_then(|(key, drag)| {
            let node_index = self.nodes.iter().position(|node| NodeKey::of(node) == key)?;
            Some(ScrollDrag { node_index, ..drag })
        });
        // A scroll not drawn yet still moves the list in the panel; rows built for it draw as they come into view.
        self.scroll_move = scrolled.and_then(|(key, scroll_move)| {
            let node_index = self.nodes.iter().position(|node| NodeKey::of(node) == key)?;
            Some(scroll_move.for_node(node_index))
        });
        self.screen_expressions = self.expressions_on_screen();
        self.nodes_out_of_date = false;
        self.fill_app_lists(system);
        first_failure
    }

    /// Works out a new node's properties: constants from the image, expressions by running them.
    fn fill_node(&mut self, node_index: usize, now: Value, system: &impl SystemValues) -> Option<AppFailure> {
        let widget = self.tree.widget(self.nodes[node_index].widget_index)?;
        let mut first_failure = None;
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
                    let failure = self.evaluate_into(node_index, entry.property, expression, now, system);
                    first_failure = first_failure.or(failure);
                }
                PropertySource::Handler(_) => {}
            }
        }
        first_failure
    }

    /// One bit for each expression a node of the screen reads, and each repeated row's `visible`.
    fn expressions_on_screen(&self) -> u128 {
        let mut expressions = 0u128;
        for node in &self.nodes {
            let Some(widget) = self.tree.widget(node.widget_index) else {
                continue;
            };
            for entry in widget.properties().flatten() {
                if let PropertySource::Expression(expression) = entry.source {
                    if (expression as usize) < MOST_BOUND_EXPRESSIONS {
                        expressions |= 1 << expression;
                    }
                }
            }
            if let Some(expression) = self.row_visibility_expression(node) {
                expressions |= 1 << expression;
            }
        }
        expressions
    }

    // ------------------------------------------------------------------------
    // Repeated rows
    // ------------------------------------------------------------------------

    /// The expression that decides whether each row of a Repeat shows, if one does.
    fn row_visibility_expression(&self, node: &Node) -> Option<u16> {
        if node.kind != WidgetKind::Repeat {
            return None;
        }
        let row = self.tree.widget(self.tree.children(node.widget_index).next()?)?;
        match row.property(Property::Visible)? {
            PropertySource::Expression(expression) if (expression as usize) < MOST_BOUND_EXPRESSIONS => {
                Some(expression)
            }
            _ => None,
        }
    }

    /// The rows of a Repeat to build nodes for: every row, or when its rows have one fixed height,
    /// only the shown rows that can be seen, noting their slots so layout leaves room for the rest.
    fn rows_to_build(&mut self, node_index: usize, now: Value, system: &impl SystemValues) -> Vec<u16> {
        let node = &self.nodes[node_index];
        let count = node.settings.value.clamp(0, u16::MAX as i32) as u16;
        let row_widget = self
            .tree
            .children(node.widget_index)
            .next()
            .and_then(|row| self.tree.widget(row));
        // Rows sit in slots only down a list that scrolls; elsewhere they are laid out as their parent's own children.
        let in_a_list = node.parent.is_some_and(|parent| {
            matches!(
                self.nodes[parent as usize].kind,
                WidgetKind::List | WidgetKind::Scroll
            )
        });
        let extent = match row_widget.and_then(|row| row.property(Property::Height)) {
            Some(PropertySource::Constant(height)) if height > 0 && in_a_list => height,
            _ => return (0..count).collect(),
        };
        let visibility = row_widget.and_then(|row| row.property(Property::Visible));
        let shown: Vec<u16> = (0..count)
            .filter(|&row| self.row_is_shown(visibility, row, now, system))
            .collect();
        let node = &self.nodes[node_index];
        let gap = i32::from(node.settings.gap);
        let (first, last) = slots_in_view(node.bounds.y, node.clip, extent, gap, shown.len() as i32);
        if let Some(Extra::Repeat {
            slot_extent,
            slot_count,
            first_built_slot,
            built_slots,
        }) = self.nodes[node_index].extra.get_mut()
        {
            *slot_extent = extent.clamp(1, i16::MAX as i32) as i16;
            *slot_count = shown.len() as u16;
            *first_built_slot = first as u16;
            *built_slots = (last - first) as u16;
        }
        shown[first as usize..last as usize].to_vec()
    }

    /// Whether one row of a Repeat shows, worked out without building it.
    fn row_is_shown(
        &mut self,
        visibility: Option<PropertySource>,
        row: u16,
        now: Value,
        system: &impl SystemValues,
    ) -> bool {
        match visibility {
            None => true,
            Some(PropertySource::Constant(value)) => value != 0,
            Some(PropertySource::Expression(expression)) => {
                let clocked = Clocked { values: system, now };
                self.interpreter.set_row(row as Value);
                self.interpreter
                    .evaluate_expression_with_library(
                        &self.image,
                        self.library.as_ref(),
                        expression,
                        &self.state,
                        &clocked,
                    )
                    .is_ok_and(|value| value != 0)
            }
            Some(PropertySource::Handler(_)) => false,
        }
    }

    /// Whether a Repeat's rows that can be seen are no longer the rows built, after a scroll or a new layout.
    fn rows_in_view_moved(&self) -> bool {
        self.nodes.iter().any(|node| {
            let Some(slots) = row_slots(node) else {
                return false;
            };
            let gap = i32::from(node.settings.gap);
            let in_view = slots_in_view(node.bounds.y, node.clip, slots.extent, gap, slots.count);
            in_view != (slots.first_built, slots.first_built + slots.built)
        })
    }

    /// Lays the screen out again, then builds the rows that came into view and lays out once more.
    fn settle_layout(&mut self, now: Value, system: &impl SystemValues) -> Option<AppFailure> {
        self.update_layout();
        let mut first_failure = None;
        for _ in 0..2 {
            if !self.rows_in_view_moved() {
                break;
            }
            first_failure = first_failure.or(self.build_nodes(now, system));
            self.update_layout();
        }
        self.fill_text_files(system);
        first_failure
    }

    fn draw_context(&self) -> DrawContext<'_, 'image> {
        DrawContext {
            image: &self.image,
            library: self.library.as_ref(),
            installed_app_icons: self.installed_app_icons,
            state: &self.state,
            theme: &self.theme,
            utc_offset: self.utc_offset,
            now: self.now,
            clip: SCREEN_AREA,
        }
    }

    /// Works out what each widget's images and arrays want, then places every widget.
    fn measure_and_lay_out(&mut self) {
        let mut measured = core::mem::take(&mut self.nodes);
        {
            let context = self.draw_context();
            for node in &mut measured {
                measure_node(node, &context);
            }
        }
        lay_out(&mut measured, SCREEN_AREA);
        self.nodes = measured;
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
        self.interpreter.set_row(self.nodes[node_index].row as Value);
        let result = self.interpreter.evaluate_expression_with_library(
            &self.image,
            self.library.as_ref(),
            expression,
            &self.state,
            &clocked,
        );
        let failed = |failure| Some(AppFailure::Expression { expression, failure });

        let value = match result {
            Ok(value) => value,
            // Until the host has the library, what calls it waits, and is worked out once it arrives.
            Err(Failure::NoLibrary) => return None,
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
            if property == Property::Visible {
                self.nodes_out_of_date = true;
            }
        }
        None
    }

    /// Works out again every property bound to one of `expressions`, marks widgets showing a changed
    /// array, then redoes the layout.
    fn refresh(&mut self, expressions: u128, changed_variables: u64, now: Value, system: &mut impl System) {
        if !self.is_open {
            return;
        }
        let expressions = expressions & self.screen_expressions;
        for (node_index, node) in self.nodes.iter().enumerate() {
            if node.arrays_read() & changed_variables != 0 {
                self.dirty_nodes |= 1 << node_index;
            }
        }

        let mut first_failure = None;
        if expressions != 0 {
            for node_index in 0..self.nodes.len() {
                let Some(widget) = self.tree.widget(self.nodes[node_index].widget_index) else {
                    continue;
                };
                for entry in widget.properties().flatten() {
                    let PropertySource::Expression(expression) = entry.source else {
                        continue;
                    };
                    if (expression as usize) < MOST_BOUND_EXPRESSIONS && expressions & (1 << expression) != 0
                    {
                        let failure = self.evaluate_into(node_index, entry.property, expression, now, system);
                        first_failure = first_failure.or(failure);
                    }
                }
            }
        }

        let rows_changed = self
            .nodes
            .iter()
            .filter_map(|node| self.row_visibility_expression(node))
            .any(|expression| expressions & (1 << expression) != 0);
        if self.nodes_out_of_date || rows_changed {
            first_failure = first_failure.or(self.build_nodes(now, system));
        }
        first_failure = first_failure.or(self.settle_layout(now, system));
        if let Some(failure) = first_failure {
            system.app_failed(failure);
        }
    }

    /// Lays the screen out again, and marks every node that moved, appeared or vanished.
    fn update_layout(&mut self) {
        if self.dirty_nodes == 0 {
            return;
        }
        let before: Vec<(Rect, bool, Rect)> = self
            .nodes
            .iter()
            .map(|node| (node.bounds, node.shown, node.clip))
            .collect();
        self.measure_and_lay_out();
        for (node_index, node) in self.nodes.iter().enumerate() {
            if before[node_index] != (node.bounds, node.shown, node.clip) {
                self.dirty_nodes |= 1 << node_index;
            }
        }
    }

    // ------------------------------------------------------------------------
    // Running handlers
    // ------------------------------------------------------------------------

    /// Runs a handler, carries out its effects and redraws what its changes affect.
    #[allow(clippy::too_many_arguments)]
    fn run_handler(
        &mut self,
        handler: u16,
        event: EventKind,
        fields: &[Value],
        texts: &[&str],
        now: Value,
        system: &mut impl System,
        may_navigate: bool,
        row: u16,
    ) -> EventResult {
        self.note_time(now, system);
        self.interpreter.set_row(row as Value);
        let clocked = Clocked {
            values: &*system,
            now,
        };
        let outcome = match self.interpreter.run_handler_with_library(
            &self.image,
            self.library.as_ref(),
            handler,
            &mut self.state,
            fields,
            texts,
            &clocked,
        ) {
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

        let may_navigate = may_navigate && self.is_open;
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

        self.after_change(outcome.changed_variables(), 0, now, system);
        if let Some(navigation) = navigation {
            self.navigate(navigation, now, system);
        }
        EventResult::Handled
    }

    /// Follows a change of state or system values: derives, saving, and redrawing what shows them.
    fn after_change(
        &mut self,
        changed_variables: u64,
        changed_system_values: u32,
        now: Value,
        system: &mut impl System,
    ) {
        let changed = Inputs {
            state_slots: changed_variables,
            system_values: changed_system_values,
        };
        let derived = self.run_derives(changed, false, now, system);
        let all_variables = changed_variables | derived;
        self.changed_persistent_variables |= all_variables & self.persistent_variables;
        self.unsaved_persistent_variables |= all_variables & self.persistent_variables;

        let affected = mask_of(self.bindings.affected_by(Inputs {
            state_slots: all_variables,
            system_values: changed_system_values,
        }));
        self.refresh(affected, all_variables, now, system);
        if changed_system_values & SystemValue::InstalledAppCount.bit() != 0 && self.fill_app_lists(system) {
            self.update_layout();
        }
    }

    /// Reads the installed apps' names into every AppList on the screen; true when any changed.
    fn fill_app_lists(&mut self, system: &impl SystemValues) -> bool {
        let mut changed = false;
        for (node_index, node) in self.nodes.iter_mut().enumerate() {
            if node.kind != WidgetKind::AppList {
                continue;
            }
            let count = system
                .read(SystemValue::InstalledAppCount)
                .clamp(0, i32::MAX as Value);
            let mut names = String::new();
            for place in 0..count {
                if place > 0 {
                    names.push('\n');
                }
                names.push_str(system.read_element_text(SystemValue::InstalledAppName, place));
            }
            if node.settings.value as Value != count || node.text != names {
                node.settings.value = count as i32;
                node.text = names;
                self.dirty_nodes |= 1 << node_index;
                changed = true;
            }
        }
        changed
    }

    /// Reads into each TextFile the lines in view, wrapping its file first when its width or font is new.
    ///
    /// Kept out of line, so its frame is on the stack only while it runs, not under every layout.
    #[inline(never)]
    fn fill_text_files(&mut self, system: &impl SystemValues) {
        for node_index in 0..self.nodes.len() {
            let node = &self.nodes[node_index];
            if node.kind != WidgetKind::TextFile || !node.shown {
                continue;
            }
            let key = NodeKey::of(node);
            let Some(widget) = self.tree.widget(node.widget_index) else {
                continue;
            };
            let text_of = |property| match widget.property(property) {
                Some(PropertySource::Constant(string)) => self.image.string(string as u16),
                _ => None,
            };
            let Some(path) = text_of(Property::Path) else {
                continue;
            };
            let font_path = text_of(Property::FontPath);
            let mut read_file = |offset: u32, out: &mut [u8]| system.read_file(path, offset, out);
            let place = self.text_files.iter().position(|(held, _)| *held == key);

            let node = &mut self.nodes[node_index];
            if place.is_none() {
                let chosen_font = font_path.and_then(|font_path| {
                    font_from_file(&mut |offset: u32, out: &mut [u8]| {
                        system.read_file(font_path, offset, out)
                    })
                });
                if let Some(font) = chosen_font {
                    node.settings.font = font;
                }
            }
            let padding = i32::from(node.settings.padding);
            let width = (node.bounds.width - 2 * padding).max(1);
            let font = node.settings.font;
            let index_fits = place.is_some_and(|place| {
                let index = &self.text_files[place].1;
                index.width == width && index.font == font
            });
            let place = match place {
                Some(place) if index_fits => place,
                _ => {
                    // The scroll offset is 16 bits, so the lines past what it reaches are left out.
                    let most_lines = (i16::MAX as i32 - 2 * padding) / line_height(font.metrics()).max(1);
                    let index = LineIndex::build(&mut read_file, font, width, most_lines);
                    self.text_files.retain(|(held, _)| *held != key);
                    self.text_files.push((key, index));
                    self.dirty_nodes |= 1 << node_index;
                    self.text_files.len() - 1
                }
            };
            let index = &self.text_files[place].1;
            if index.line_count == 0 {
                let placeholder = text_of(Property::Placeholder).unwrap_or("");
                let lines: Vec<&str> = WrappedLines::new(placeholder, font, width).collect();
                node.text = lines.join("\n");
                if let Some(extra) = node.extra.get_mut() {
                    *extra = Extra::TextFile {
                        line_count: lines.len() as u16,
                        first_line: 0,
                        held_lines: lines.len() as u16,
                        is_placeholder: true,
                    };
                }
                continue;
            }
            let (view_first, view_end) = text_file_lines_in_view(node);
            let Some(&mut Extra::TextFile {
                line_count,
                first_line,
                held_lines,
                ..
            }) = node.extra.get_mut()
            else {
                continue;
            };
            let holds_view = index_fits
                && covers(
                    i32::from(first_line),
                    i32::from(held_lines),
                    view_first,
                    view_end,
                    i32::from(line_count),
                );
            if holds_view {
                continue;
            }
            let (window_first, text) = index.read_lines(&mut read_file, view_first, view_end);
            let lines = if text.is_empty() {
                0
            } else {
                text.split('\n').count()
            };
            node.text = text;
            if let Some(extra) = node.extra.get_mut() {
                *extra = Extra::TextFile {
                    line_count: index.line_count as u16,
                    first_line: window_first as u16,
                    held_lines: lines as u16,
                    is_placeholder: false,
                };
            }
        }
    }

    /// Runs the derive subscriptions whose inputs changed, or all of them; returns what they changed.
    ///
    /// A derive's output may feed a later one, so they run in order, and again while anything changes.
    fn run_derives(&mut self, changed: Inputs, run_all: bool, now: Value, system: &impl SystemValues) -> u64 {
        let mut pending = changed;
        let mut changed_by_derives = 0u64;
        for _ in 0..self.subscriptions.len().max(1) {
            let mut changed_this_round = 0u64;
            for index in 0..self.subscriptions.len() {
                let memory = self.subscriptions[index];
                if memory.subscription.kind != SubscriptionKind::Derive || memory.is_turned_off() {
                    continue;
                }
                if !run_all && !memory.derive_inputs.overlaps(pending) {
                    continue;
                }
                let clocked = Clocked { values: system, now };
                self.interpreter.set_row(0);
                let outcome = self.interpreter.run_handler_with_library(
                    &self.image,
                    self.library.as_ref(),
                    memory.subscription.handler,
                    &mut self.state,
                    &[],
                    &[],
                    &clocked,
                );
                match outcome {
                    Ok(outcome) => {
                        self.subscriptions[index].failures_in_a_row = 0;
                        changed_this_round |= outcome.changed_variables();
                    }
                    Err(Failure::NoLibrary) => {}
                    Err(_) => self.subscriptions[index].failures_in_a_row += 1,
                }
            }
            changed_by_derives |= changed_this_round;
            if changed_this_round == 0 || run_all {
                break;
            }
            pending = Inputs::state_slot(0);
            pending.state_slots = changed_this_round;
        }
        changed_by_derives
    }

    // ------------------------------------------------------------------------
    // Subscriptions
    // ------------------------------------------------------------------------

    /// Runs every subscription of one kind, and detail if given, with the event's fields.
    fn run_subscriptions_of(
        &mut self,
        kind: SubscriptionKind,
        detail: Option<u8>,
        fields: &[Value],
        texts: &[&str],
        now: Value,
        system: &mut impl System,
    ) -> EventResult {
        let mut result = EventResult::NotHandled;
        for index in 0..self.subscriptions.len() {
            let subscription = self.subscriptions[index].subscription;
            if subscription.kind != kind || detail.is_some_and(|detail| detail != subscription.detail) {
                continue;
            }
            result = combine(result, self.run_subscription(index, fields, texts, now, system));
        }
        result
    }

    /// Runs one subscription's handler, unless it waits for the app to be open or keeps failing.
    fn run_subscription(
        &mut self,
        index: usize,
        fields: &[Value],
        texts: &[&str],
        now: Value,
        system: &mut impl System,
    ) -> EventResult {
        let memory = self.subscriptions[index];
        if memory.is_turned_off() || (memory.subscription.only_while_open && !self.is_open) {
            return EventResult::NotHandled;
        }
        let event = memory.subscription.kind.event();
        let result = self.run_handler(
            memory.subscription.handler,
            event,
            fields,
            texts,
            now,
            system,
            true,
            0,
        );
        let failures = &mut self.subscriptions[index].failures_in_a_row;
        *failures = match result {
            EventResult::Failed => failures.saturating_add(1).min(FAILURES_BEFORE_TURNING_OFF),
            _ => 0,
        };
        result
    }

    /// Hands the app something the system noticed, such as a notification or a step count.
    ///
    /// Acceleration readings are kept and handed to acceleration subscriptions at their period.
    pub fn deliver(&mut self, event: PushedEvent, now: Value, system: &mut impl System) -> EventResult {
        self.note_time(now, system);
        let fields = event.fields();
        match event {
            PushedEvent::Acceleration { x, y, z } => {
                self.latest_acceleration = [x, y, z];
                return EventResult::NotHandled;
            }
            PushedEvent::HeartRate {
                beats_per_minute,
                confidence,
            } => self.latest_heart_rate = [beats_per_minute, confidence],
            _ => {}
        }
        let (kind, detail) = event.subscription();
        let result = self.run_subscriptions_of(
            kind,
            detail,
            &fields.numbers[..fields.count],
            &fields.texts,
            now,
            system,
        );
        let changed_system_values = event.changed_system_values();
        if changed_system_values != 0 {
            self.after_change(0, changed_system_values, now, system);
        }
        result
    }

    /// Fires every timer-like subscription that has come due since the last check.
    fn fire_due_timers(&mut self, now: Value, system: &mut impl System) {
        let since = self.last_timer_check;
        self.last_timer_check = now;
        for index in 0..self.subscriptions.len() {
            let memory = self.subscriptions[index];
            if !memory.is_timer()
                || memory.is_turned_off()
                || (memory.subscription.only_while_open && !self.is_open)
            {
                continue;
            }
            let subscription = memory.subscription;
            let due = match subscription.kind {
                SubscriptionKind::At => match subscription.current_value(&self.state) {
                    Some(at) if at != NONE && at <= now && at != memory.fired_for => Some(at),
                    _ => None,
                },
                _ => subscription
                    .next_due(since, &self.state, self.utc_offset)
                    .filter(|&due| due <= now),
            };
            let Some(due) = due else {
                continue;
            };
            if subscription.kind == SubscriptionKind::At {
                self.subscriptions[index].fired_for = due;
            }
            let fields: &[Value] = match subscription.kind {
                SubscriptionKind::HeartRate => &self.latest_heart_rate.clone(),
                SubscriptionKind::Acceleration => &self.latest_acceleration.clone(),
                _ => &[due],
            };
            self.run_subscription(index, fields, &[], now, system);
        }
    }

    /// When the next timer-like subscription is due, if any is waiting.
    fn next_timer(&self, now: Value) -> Option<Value> {
        self.subscriptions
            .iter()
            .filter(|memory| memory.is_timer() && !memory.is_turned_off())
            .filter(|memory| self.is_open || !memory.subscription.only_while_open)
            .filter_map(|memory| {
                let subscription = memory.subscription;
                match subscription.kind {
                    SubscriptionKind::At => subscription
                        .current_value(&self.state)
                        .filter(|&at| at != NONE && at != memory.fired_for)
                        .map(|at| at.max(now)),
                    _ => subscription.next_due(now, &self.state, self.utc_offset),
                }
            })
            .min()
    }

    /// The subscriptions that would run now, leaving out failed ones and open-only ones of a closed app.
    fn running_subscriptions(&self) -> impl Iterator<Item = &Subscription> + '_ {
        self.subscriptions
            .iter()
            .filter(|memory| !memory.is_turned_off())
            .filter(|memory| self.is_open || !memory.subscription.only_while_open)
            .map(|memory| &memory.subscription)
    }

    /// Whether a running subscription waits for this event, such as `motion("shake")`, so the system watches only for what is wanted.
    pub fn listens_for(&self, kind: SubscriptionKind, detail: Option<u8>) -> bool {
        self.running_subscriptions().any(|subscription| {
            subscription.kind == kind && detail.is_none_or(|detail| detail == subscription.detail)
        })
    }

    /// The shortest period of the running subscriptions of a periodic kind, such as `acceleration`.
    pub fn shortest_period_of(&self, kind: SubscriptionKind) -> Option<Value> {
        self.running_subscriptions()
            .filter(|subscription| subscription.kind == kind)
            .filter_map(|subscription| subscription.current_value(&self.state))
            .min()
    }

    // ------------------------------------------------------------------------
    // Touches and buttons
    // ------------------------------------------------------------------------

    /// Sends a touch to the topmost widget under it that answers it, or a swipe to the screen.
    pub fn touch(&mut self, touch: Touch, now: Value, system: &mut impl System) -> EventResult {
        self.note_time(now, system);
        if !self.is_open {
            return EventResult::NotHandled;
        }
        if let Touch::Press { .. } = touch {
            self.touch_scrolled = false;
        }
        if let Some(result) = self.touch_open_dropdown(touch, now, system) {
            return result;
        }
        let (property, x, y, mut fields): (Property, i32, i32, [Value; 5]) = match touch {
            // The panel can name a slide while the finger is still down, so only a lift ends a scroll.
            Touch::Swipe(_) if self.scroll_drag.is_some() => return EventResult::Handled,
            Touch::Swipe(SwipeDirection::Up | SwipeDirection::Down) if self.touch_scrolled => {
                return EventResult::Handled
            }
            Touch::Swipe(direction) => {
                let result =
                    self.run_screen_event(Property::OnSwipe, &[swipe_value(direction)], now, system, true);
                return match (result, direction) {
                    (EventResult::NotHandled, SwipeDirection::Right) => self.go_back_a_screen(now, system),
                    _ => result,
                };
            }
            Touch::Release { .. } if self.scroll_drag.is_some() => {
                return self.end_scroll(now, system).unwrap_or(EventResult::Handled);
            }
            Touch::Click { x, y } => (Property::OnClick, x, y, [x as Value, y as Value, 0, 0, 0]),
            Touch::LongPress { x, y } => (Property::OnLongPress, x, y, [x as Value, y as Value, 0, 0, 0]),
            Touch::DoubleClick { x, y } => (Property::OnDoubleClick, x, y, [x as Value, y as Value, 0, 0, 0]),
            Touch::Press { x, y } => (Property::OnPress, x, y, [x as Value, y as Value, 0, 0, 0]),
            Touch::Release { x, y } => (Property::OnRelease, x, y, [x as Value, y as Value, 0, 0, 0]),
            Touch::Drag {
                x,
                y,
                start_x,
                start_y,
            } => (
                Property::OnDrag,
                x,
                y,
                [x as Value, y as Value, start_x as Value, start_y as Value, 0],
            ),
        };
        let event = EventKind::for_property(property).unwrap_or(EventKind::Click);
        let field_count = event.fields().len();

        let Some((node_index, handler_property, handler)) = self.hit_test(property, x, y) else {
            if let Touch::Drag { y, start_y, .. } = touch {
                return self.scroll(x, y, start_y, now, system);
            }
            return EventResult::NotHandled;
        };
        if self.nodes[node_index].kind == WidgetKind::Dropdown && handler_property == Property::OnClick {
            self.nodes[node_index].is_open = true;
            self.dirty_nodes |= 1 << node_index;
            return EventResult::Handled;
        }
        if handler_property == Property::OnChange {
            let Some(value) = self.changed_value(node_index, x, y) else {
                return EventResult::NotHandled;
            };
            let row = self.nodes[node_index].row;
            return self.run_handler(handler, EventKind::Change, &[value], &[], now, system, true, row);
        }
        let touched_value = match self.nodes[node_index].kind {
            WidgetKind::CellGrid => cell_at(&self.nodes[node_index], x, y),
            WidgetKind::AppList => app_row_at(&self.nodes[node_index], x, y),
            WidgetKind::ButtonMatrix => self.matrix_button_at(node_index, x, y),
            _ => Some(0),
        };
        let Some(touched_value) = touched_value else {
            return EventResult::NotHandled;
        };
        fields[field_count - 1] = touched_value as Value;
        let row = self.nodes[node_index].row;
        self.run_handler(
            handler,
            event,
            &fields[..field_count],
            &[],
            now,
            system,
            true,
            row,
        )
    }

    /// Sends a side button press to the screen's `onButton`.
    pub fn press_button(&mut self, press: ButtonPress, now: Value, system: &mut impl System) -> EventResult {
        if !self.is_open {
            return EventResult::NotHandled;
        }
        match self.run_screen_event(Property::OnButton, &[button_value(press)], now, system, true) {
            EventResult::NotHandled => self.go_back_a_screen(now, system),
            result => result,
        }
    }

    /// Returns to the previous screen, as an unused button or right swipe does; not handled on the first screen.
    fn go_back_a_screen(&mut self, now: Value, system: &mut impl System) -> EventResult {
        if self.screen_history_length == 0 {
            return EventResult::NotHandled;
        }
        self.navigate(Navigation::Back, now, system);
        EventResult::Handled
    }

    /// Whether a node, or any widget around it, is disabled.
    fn is_disabled(&self, node_index: usize) -> bool {
        let mut next = Some(node_index);
        while let Some(index) = next {
            if self.nodes[index].settings.disabled {
                return true;
            }
            next = self.nodes[index].parent.map(|parent| parent as usize);
        }
        false
    }

    /// The topmost shown widget under a point that answers the event, or an input's `onChange`.
    fn hit_test(&self, property: Property, x: i32, y: i32) -> Option<(usize, Property, u16)> {
        let is_inside = |bounds: Rect| {
            x >= bounds.x && x < bounds.x + bounds.width && y >= bounds.y && y < bounds.y + bounds.height
        };
        for (node_index, node) in self.nodes.iter().enumerate().rev() {
            let Some(area) = node.covered_area() else {
                continue;
            };
            if !is_inside(area) || self.is_disabled(node_index) {
                continue;
            }
            let Some(widget) = self.tree.widget(node.widget_index) else {
                continue;
            };
            if let Some(PropertySource::Handler(handler)) = widget.property(property) {
                return Some((node_index, property, handler));
            }
            let sends_change = match node.kind {
                WidgetKind::Switch | WidgetKind::Checkbox | WidgetKind::Radio | WidgetKind::Roller => {
                    property == Property::OnClick
                }
                kind if is_picker(kind) => property == Property::OnClick,
                WidgetKind::Slider => matches!(property, Property::OnClick | Property::OnDrag),
                WidgetKind::Dropdown if property == Property::OnClick => {
                    return Some((node_index, property, 0))
                }
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

    /// The value a tap gives an input: a switch or checkbox flipped, a radio chosen, a slider's
    /// value under `x`, or the option tapped in a roller.
    fn changed_value(&self, node_index: usize, x: i32, y: i32) -> Option<Value> {
        let node = &self.nodes[node_index];
        let settings = &node.settings;
        Some(match node.kind {
            kind if is_picker(kind) => return picker_tapped(node, x, y),
            WidgetKind::Switch | WidgetKind::Checkbox => (settings.value == 0) as Value,
            WidgetKind::Radio => 1,
            WidgetKind::Roller => {
                let rows = roller_rows(node);
                let row_height = option_row_height(node).max(1);
                let top = node.bounds.y + (node.bounds.height - rows * row_height) / 2;
                let row = (y - top).div_euclid(row_height).clamp(0, rows - 1);
                let last = self.array_length(settings.array).saturating_sub(1) as i32;
                (settings.value + row - rows / 2).clamp(0, last) as Value
            }
            _ => {
                let width = node.bounds.width.max(1) as Value;
                let along = (x - node.bounds.x).clamp(0, node.bounds.width) as Value;
                let span = settings.maximum as Value - settings.minimum as Value;
                settings.minimum as Value + span * along / width
            }
        })
    }

    fn array_length(&self, variable: Option<u8>) -> usize {
        variable
            .and_then(|variable| self.state.placement(variable))
            .map_or(0, |placement| placement.length as usize)
    }

    fn matrix_button_at(&self, node_index: usize, x: i32, y: i32) -> Option<i32> {
        let context = self.draw_context();
        let node = &self.nodes[node_index];
        let found = matrix_buttons(node, &context)
            .find(|(_, place, _)| {
                x >= place.x && x < place.x + place.width && y >= place.y && y < place.y + place.height
            })
            .map(|(number, _, _)| number);
        found
    }

    /// An open Dropdown takes the next touch: a tap on an option chooses it, anything else closes it.
    fn touch_open_dropdown(
        &mut self,
        touch: Touch,
        now: Value,
        system: &mut impl System,
    ) -> Option<EventResult> {
        let node_index = self.nodes.iter().position(|node| node.is_open)?;
        if matches!(
            touch,
            Touch::Drag { .. } | Touch::Press { .. } | Touch::Release { .. }
        ) {
            return Some(EventResult::Handled);
        }
        self.nodes[node_index].is_open = false;
        self.dirty_nodes |= 1 << node_index;
        let Touch::Click { x, y } = touch else {
            return Some(EventResult::Handled);
        };
        let node = &self.nodes[node_index];
        let inside_across = x >= node.bounds.x && x < node.bounds.x + node.bounds.width;
        let option =
            dropdown_option_at(node, y).filter(|&option| inside_across && option < node.option_count as i32);
        let row = node.row;
        let handler = self
            .tree
            .widget(node.widget_index)
            .and_then(|widget| widget.property(Property::OnChange));
        match (option, handler) {
            (Some(option), Some(PropertySource::Handler(handler))) => Some(self.run_handler(
                handler,
                EventKind::Change,
                &[option as Value],
                &[],
                now,
                system,
                true,
                row,
            )),
            _ => Some(EventResult::Handled),
        }
    }

    /// Scrolls the Scroll or List under a drag, keeping its content inside it.
    fn scroll(&mut self, x: i32, y: i32, start_y: i32, now: Value, system: &mut impl System) -> EventResult {
        let drag = match self.scroll_drag {
            Some(drag) => drag,
            None => {
                let found = self.nodes.iter().enumerate().rev().find(|(index, node)| {
                    clips_children(node.kind)
                        && node.covered_area().is_some_and(|area| {
                            x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
                        })
                        && !self.is_disabled(*index)
                });
                let Some((node_index, node)) = found else {
                    return EventResult::NotHandled;
                };
                ScrollDrag {
                    node_index,
                    start_offset: node.scroll_offset as i32,
                }
            }
        };
        self.scroll_drag = Some(drag);
        let farthest =
            (content_height(&self.nodes, drag.node_index) - self.nodes[drag.node_index].bounds.height).max(0);
        let offset = (drag.start_offset - (y - start_y)).clamp(0, farthest);
        let node = &mut self.nodes[drag.node_index];
        let moved_rows = offset - node.scroll_offset as i32;
        if moved_rows != 0 {
            node.scroll_offset = offset as i16;
            self.touch_scrolled = true;
            self.fill_text_files(&*system);
            self.note_scroll(drag.node_index, moved_rows);
            if self.rows_in_view_moved() {
                let failure = self.build_nodes(now, system);
                self.measure_and_lay_out();
                let failure = failure.or(self.settle_layout(now, system));
                if let Some(failure) = failure {
                    system.app_failed(failure);
                }
            }
        }
        EventResult::Handled
    }

    /// Ends a scroll and sends `onScrollEnd` with the offset, if the scrolled widget has one.
    fn end_scroll(&mut self, now: Value, system: &mut impl System) -> Option<EventResult> {
        let drag = self.scroll_drag.take()?;
        let node = self.nodes.get(drag.node_index)?;
        let offset = node.scroll_offset as Value;
        let handler = self
            .tree
            .widget(node.widget_index)
            .and_then(|widget| widget.property(Property::OnScrollEnd));
        Some(match handler {
            Some(PropertySource::Handler(handler)) => {
                let row = self.nodes[drag.node_index].row;
                self.run_handler(
                    handler,
                    EventKind::ScrollEnd,
                    &[offset],
                    &[],
                    now,
                    system,
                    true,
                    row,
                )
            }
            _ => EventResult::Handled,
        })
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
        self.run_handler(handler, event, fields, &[], now, system, may_navigate, 0)
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

        self.next_transition = match navigation {
            Navigation::Go(_) => Transition::Left,
            Navigation::Back => Transition::Right,
        };
        self.run_screen_event(Property::OnHide, &[], now, system, false);
        self.show_screen(next_screen, now, system);
        self.run_screen_event(Property::OnShow, &[], now, system, false);
    }

    // ------------------------------------------------------------------------
    // Time, system values and theme
    // ------------------------------------------------------------------------

    /// Fires the subscriptions that came due, and redraws whatever the clock has changed on screen.
    pub fn tick(&mut self, now: Value, system: &mut impl System) {
        self.note_time(now, system);
        self.fire_due_timers(now, system);
        if !self.is_open {
            return;
        }
        let due = mask_of(self.bindings.due_between_among(
            self.last_clock_update,
            now,
            self.screen_expressions,
        ));
        self.last_clock_update = now;
        for (node_index, node) in self.nodes.iter().enumerate() {
            if node.shown && label_scrolls(node) {
                self.dirty_nodes |= 1 << node_index;
            }
        }
        self.refresh(due, 0, now, system);
    }

    /// When the host next has work: a clock redraw, a scrolling label or a subscription coming due.
    pub fn next_wakeup(&self, now: Value) -> Option<Value> {
        let redraw = match self.is_open {
            true => self
                .bindings
                .next_redraw_after_among(now, self.screen_expressions),
            false => None,
        };
        let scrolling = (self.is_open && self.nodes.iter().any(|node| node.shown && label_scrolls(node)))
            .then_some(now + LABEL_SCROLL_STEP_IN_MILLISECONDS);
        [redraw, scrolling, self.next_timer(now)]
            .into_iter()
            .flatten()
            .min()
    }

    /// When the next subscription comes due, leaving out redraws, for while the screen is dark.
    pub fn next_subscription_due(&self, now: Value) -> Option<Value> {
        self.next_timer(now)
    }

    /// Redraws what reads the system values that changed, given as `SystemValue::bit` values or'd together.
    pub fn system_values_changed(&mut self, changed: u32, now: Value, system: &mut impl System) {
        self.note_time(now, system);
        self.after_change(0, changed, now, system);
    }

    /// Redraws what reads the changed system values, then pushes the events some of them stand for.
    ///
    /// A new step count pushes `steps`, and the phone connecting or going away pushes `phone`, with
    /// the values read from `system`.
    pub fn report_system_value_changes(&mut self, changed: u32, now: Value, system: &mut impl System) {
        self.system_values_changed(changed, now, system);
        if changed & SystemValue::StepsToday.bit() != 0 {
            let steps_today = system.read(SystemValue::StepsToday);
            self.deliver(PushedEvent::Steps { steps_today }, now, system);
        }
        if changed & SystemValue::IsPhoneConnected.bit() != 0 {
            let is_connected = system.read(SystemValue::IsPhoneConnected) != 0;
            self.deliver(PushedEvent::Phone { is_connected }, now, system);
        }
    }

    /// Changes the theme and redraws every colour, without running any app code.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.redraw_whole_screen();
    }

    /// Draws the whole screen next, starting any transition again from its beginning.
    fn redraw_whole_screen(&mut self) {
        self.whole_screen_dirty = true;
        self.transition_run = None;
    }

    // ------------------------------------------------------------------------
    // Drawing
    // ------------------------------------------------------------------------

    /// Whether `draw` has anything to do.
    pub fn needs_drawing(&self) -> bool {
        self.is_open
            && (self.whole_screen_dirty
                || self.dirty_nodes != 0
                || !self.cleared_regions.is_empty()
                || self.scroll_move.is_some())
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

    /// Each node of a widget, as the part of it that can be seen and its text: one for most widgets,
    /// one for each built row of a repeated list. The part is none when the node is hidden or out of sight.
    pub fn widget_nodes(&self, widget_index: u16) -> Vec<(Option<Rect>, &str)> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.widget_index == widget_index)
            .map(|(node_index, node)| (self.seen_area(node_index), node.text()))
            .collect()
    }

    /// The part of a node inside the screen and every node around it, if it is shown.
    fn seen_area(&self, node_index: usize) -> Option<Rect> {
        let node = &self.nodes[node_index];
        if !node.shown {
            return None;
        }
        let mut area = node.bounds.intersect(SCREEN_AREA)?;
        let mut next = node.parent;
        while let Some(parent) = next {
            let parent = &self.nodes[parent as usize];
            area = area.intersect(parent.bounds)?;
            next = parent.parent;
        }
        (area.width > 0 && area.height > 0).then_some(area)
    }

    /// The text each node of a widget shows: one for most widgets, one for each built row of a repeated list.
    pub fn widget_texts(&self, widget_index: u16) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|node| node.widget_index == widget_index)
            .map(|node| node.text())
            .collect()
    }

    /// The rows a list widget such as an AppList or a TextFile draws itself, each with its text; empty for other widgets.
    pub fn widget_rows(&self, widget_index: u16) -> Vec<(&str, Rect)> {
        match self.node(widget_index) {
            Some(node) if node.kind == WidgetKind::AppList => {
                app_list_rows(node).map(|(_, name, row)| (name, row)).collect()
            }
            Some(node) if node.kind == WidgetKind::TextFile => {
                let height = line_height(node.settings.font.metrics());
                let row = |top| Rect {
                    x: node.bounds.x + i32::from(node.settings.padding),
                    y: top,
                    width: (node.bounds.width - 2 * i32::from(node.settings.padding)).max(0),
                    height,
                };
                text_file_rows(node).map(|(line, top)| (line, row(top))).collect()
            }
            _ => Vec::new(),
        }
    }

    /// How many widgets the open screen has, which is how many nodes the UI heap holds.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    fn node(&self, widget_index: u16) -> Option<&Node> {
        self.nodes
            .iter()
            .map(|node| &**node)
            .find(|node| node.widget_index == widget_index)
    }
}

/// What several handlers' results add up to: a failure wins, then any handler that ran.
fn combine(first: EventResult, second: EventResult) -> EventResult {
    match (first, second) {
        (EventResult::Failed, _) | (_, EventResult::Failed) => EventResult::Failed,
        (EventResult::Handled, _) | (_, EventResult::Handled) => EventResult::Handled,
        _ => EventResult::NotHandled,
    }
}
