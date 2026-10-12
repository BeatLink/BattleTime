//! Drawing the open screen: only the parts that changed, a strip of rows at a time.
//!
//! Each part is drawn into a strip in memory, background first and every widget on top, and the
//! strip is then sent to the panel at once, so the panel never shows a half-drawn widget. A
//! scrolled list is moved by the panel itself when it can, and only the rows that come into view
//! are drawn. A new screen comes in by a transition: a slide moves the panel's rows, as InfiniTime
//! does, and a wipe draws the new screen a band of columns at a time.

use alloc::{vec, vec::Vec};

use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use neotime_runtime::ThemeColor;
use neotime_ui::{FixedScreen, ScreenRenderer, ScrollBand, ScrollingScreen, Strip};
use rlvgl_core::{
    renderer::{ClipRenderer, Renderer},
    widget::{Color, Rect},
};

use super::{AppHost, DISABLED_OPACITY, SCREEN_AREA};
use crate::drawing::{draw_node, FadedRenderer};
use crate::layout::{clips_children, content_height};
use crate::nodes::{to_color, OPAQUE};

// ============================================================================
// Limits
// ============================================================================

/// How many pixels the strip `draw_step` draws into holds: four rows of the screen.
pub const STRIP_PIXELS: usize = 960;

/// How many rows a slide moves the screen at each step.
const SLIDE_STEP_ROWS: i32 = 8;

/// How many columns a wipe draws at each step.
const WIPE_STEP_COLUMNS: i32 = 16;

const SCREEN_SIDE: i32 = 240;

// ============================================================================
// Transitions
// ============================================================================

/// How a new screen replaces the old one, named by the way the new one moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transition {
    /// Drawn in place at once.
    #[default]
    None,
    /// Slides in from the bottom, pushing the old screen up.
    Up,
    /// Slides in from the top, pushing the old screen down.
    Down,
    /// Wipes in from the right edge towards the left.
    Left,
    /// Wipes in from the left edge towards the right.
    Right,
}

/// A transition under way, and how many rows or columns of the new screen it has shown.
#[derive(Clone, Copy)]
pub(super) struct TransitionRun {
    kind: Transition,
    done: i32,
}

/// Scrolling not drawn yet: the Scroll or List, and how many rows its content moved up.
#[derive(Clone, Copy)]
pub(super) struct ScrollMove {
    pub(super) node_index: usize,
    rows: i32,
}

impl ScrollMove {
    /// The same move, for the list's node at its new place after the nodes were built again.
    pub(super) fn for_node(self, node_index: usize) -> ScrollMove {
        ScrollMove { node_index, ..self }
    }
}

fn rows_from(first_row: i32, row_count: i32) -> Rect {
    Rect {
        x: 0,
        y: first_row,
        width: SCREEN_SIDE,
        height: row_count,
    }
}

fn columns_from(first_column: i32, column_count: i32) -> Rect {
    Rect {
        x: first_column,
        y: 0,
        width: column_count,
        height: SCREEN_SIDE,
    }
}

impl<'image> AppHost<'image> {
    // ------------------------------------------------------------------------
    // Drawing
    // ------------------------------------------------------------------------

    /// Sets how the next new screen comes in, such as when the app opens.
    pub fn set_next_transition(&mut self, transition: Transition) {
        self.next_transition = transition;
    }

    /// Whether a transition is part way through, so `draw_step` has more to do.
    pub fn is_in_transition(&self) -> bool {
        self.transition_run.is_some()
    }

    /// Draws everything that changed onto a screen that cannot scroll, transitions and all.
    pub fn draw<Screen>(&mut self, screen: &mut Screen) -> Result<(), Screen::Error>
    where
        Screen: DrawTarget<Color = Rgb565>,
    {
        let mut strip = vec![Rgb565::BLACK; STRIP_PIXELS];
        let mut screen = FixedScreen::new(screen);
        while self.needs_drawing() {
            self.draw_step(&mut screen, &mut strip)?;
        }
        Ok(())
    }

    /// Draws what changed since the last call, or one step of a transition, through `strip`.
    ///
    /// The strip must hold at least one row of the screen; `STRIP_PIXELS` is the size the watch uses.
    pub fn draw_step<Screen>(
        &mut self,
        screen: &mut Screen,
        strip: &mut [Rgb565],
    ) -> Result<(), Screen::Error>
    where
        Screen: ScrollingScreen,
    {
        if !self.is_open {
            return Ok(());
        }
        if self.whole_screen_dirty {
            return self.draw_whole_screen_step(screen, strip);
        }
        if let Some(scroll_move) = self.scroll_move.take() {
            self.draw_scroll(screen, strip, scroll_move)?;
        }

        let regions = self.dirty_regions();
        for region in &regions {
            self.paint(screen, strip, *region, 0)?;
        }
        for (node_index, node) in self.nodes.iter_mut().enumerate() {
            if self.dirty_nodes & (1 << node_index) != 0 {
                node.drawn_bounds = node.covered_area();
            }
        }
        self.dirty_nodes = 0;
        self.cleared_regions.clear();
        Ok(())
    }

    /// The parts of the screen to paint again: where each changed widget was and where it is now.
    fn dirty_regions(&self) -> Vec<Rect> {
        let mut regions: Vec<Rect> = Vec::new();
        for (node_index, node) in self.nodes.iter().enumerate() {
            if self.dirty_nodes & (1 << node_index) == 0 {
                continue;
            }
            for region in [node.drawn_bounds, node.covered_area()].into_iter().flatten() {
                add_region(&mut regions, region);
            }
        }
        for region in &self.cleared_regions {
            add_region(&mut regions, *region);
        }
        regions
    }

    /// Paints `area` a strip at a time, each strip sent `rows_down` lower on the screen than it belongs.
    fn paint<Screen>(
        &self,
        screen: &mut Screen,
        strip: &mut [Rgb565],
        area: Rect,
        rows_down: i32,
    ) -> Result<(), Screen::Error>
    where
        Screen: ScrollingScreen,
    {
        let Some(area) = area.intersect(SCREEN_AREA) else {
            return Ok(());
        };
        let rows_per_strip = Strip::rows_that_fit(strip.len(), area.width);
        let end_row = area.y + area.height;
        let mut first_row = area.y;

        while first_row < end_row {
            let part = Rect {
                y: first_row,
                height: rows_per_strip.min(end_row - first_row),
                ..area
            };
            let mut target = Strip::new(strip, part);
            self.render(&mut target, part);
            let destination = Rectangle::new(
                Point::new(part.x, part.y + rows_down),
                Size::new(part.width as u32, part.height as u32),
            );
            screen.fill_contiguous(&destination, target.pixels())?;
            first_row += part.height;
        }
        Ok(())
    }

    /// Draws the background and every shown widget that reaches into `region`; open lists last.
    fn render(&self, target: &mut Strip, region: Rect) {
        let background = to_color(self.theme.color(ThemeColor::Background));
        let mut renderer = ScreenRenderer::new(&mut *target).with_background_color(background);
        renderer.fill_rect(region, background);
        renderer.finish().unwrap_or_else(|never| match never {});

        let context = self.draw_context();
        let in_front_last = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| !node.is_open)
            .chain(self.nodes.iter().enumerate().filter(|(_, node)| node.is_open));
        for (node_index, node) in in_front_last {
            let Some(clip) = node.covered_area().and_then(|area| area.intersect(region)) else {
                continue;
            };
            let backdrop = self.backdrop(node_index, background);
            let opacity = self.opacity(node_index);
            let mut renderer = ScreenRenderer::new(&mut *target).with_background_color(backdrop);
            {
                let mut clipped = ClipRenderer::new(&mut renderer, clip);
                if opacity == OPAQUE {
                    draw_node(node, &context, &mut clipped);
                } else {
                    let mut faded = FadedRenderer {
                        inner: &mut clipped,
                        opacity,
                    };
                    draw_node(node, &context, &mut faded);
                }
            }
            renderer.finish().unwrap_or_else(|never| match never {});
        }
    }

    /// How opaque a node draws: its own `opacity` times every parent's, halved if it is disabled.
    fn opacity(&self, node_index: usize) -> u8 {
        let mut opacity = OPAQUE as u32;
        let mut next = Some(node_index);
        while let Some(index) = next {
            opacity = opacity * self.nodes[index].settings.opacity as u32 / OPAQUE as u32;
            next = self.nodes[index].parent.map(|parent| parent as usize);
        }
        if self.is_disabled(node_index) {
            opacity = opacity * DISABLED_OPACITY as u32 / OPAQUE as u32;
        }
        opacity as u8
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

    // ------------------------------------------------------------------------
    // A whole new screen
    // ------------------------------------------------------------------------

    /// Draws the whole screen at once, or the next step of its transition.
    fn draw_whole_screen_step<Screen>(
        &mut self,
        screen: &mut Screen,
        strip: &mut [Rgb565],
    ) -> Result<(), Screen::Error>
    where
        Screen: ScrollingScreen,
    {
        let mut run = match self.transition_run {
            Some(run) => run,
            None => self.start_whole_screen(screen)?,
        };
        let left = SCREEN_SIDE - run.done;
        let step = match run.kind {
            Transition::None => {
                self.paint(screen, strip, SCREEN_AREA, 0)?;
                left
            }
            Transition::Up => {
                let rows = SLIDE_STEP_ROWS.min(left);
                // The next rows go over the top rows, which leave next, and the panel moves them round to the bottom.
                self.paint(screen, strip, rows_from(run.done, rows), -run.done)?;
                screen.scroll_by(rows)?;
                rows
            }
            Transition::Down => {
                let rows = SLIDE_STEP_ROWS.min(left);
                self.paint(screen, strip, rows_from(left - rows, rows), run.done)?;
                screen.scroll_by(-rows)?;
                rows
            }
            Transition::Left => {
                let columns = WIPE_STEP_COLUMNS.min(left);
                self.paint(screen, strip, columns_from(left - columns, columns), 0)?;
                columns
            }
            Transition::Right => {
                let columns = WIPE_STEP_COLUMNS.min(left);
                self.paint(screen, strip, columns_from(run.done, columns), 0)?;
                columns
            }
        };

        run.done += step;
        if run.done >= SCREEN_SIDE {
            self.transition_run = None;
            self.whole_screen_dirty = false;
        } else {
            self.transition_run = Some(run);
        }
        Ok(())
    }

    /// Starts drawing a whole new screen: every widget will be drawn, and the panel is made ready.
    fn start_whole_screen<Screen>(&mut self, screen: &mut Screen) -> Result<TransitionRun, Screen::Error>
    where
        Screen: ScrollingScreen,
    {
        let wanted = core::mem::take(&mut self.next_transition);
        let slides = matches!(wanted, Transition::Up | Transition::Down);
        let kind = match slides && !screen.can_scroll() {
            true => Transition::None,
            false => wanted,
        };
        self.prepare_scroll_band(screen, kind)?;

        for node in &mut self.nodes {
            node.drawn_bounds = node.covered_area();
        }
        self.dirty_nodes = 0;
        self.cleared_regions.clear();
        self.scroll_move = None;
        Ok(TransitionRun { kind, done: 0 })
    }

    /// Readies the panel's band for a new screen, unturning its ring while all of it is drawn again anyway.
    ///
    /// A slide needs the whole screen to scroll. The screen's list needs its own band, which can
    /// only be set later while the ring is unturned, and this is the last chance to unturn it.
    fn prepare_scroll_band<Screen>(&self, screen: &mut Screen, kind: Transition) -> Result<(), Screen::Error>
    where
        Screen: ScrollingScreen,
    {
        if !screen.can_scroll() {
            return Ok(());
        }
        let state = screen.scroll_state();
        let slides = matches!(kind, Transition::Up | Transition::Down);
        let wanted = self.scrolling_list_band();
        let must_scroll_whole_screen = slides && state.band != ScrollBand::WHOLE_SCREEN;
        let must_unturn = state.turned_rows != 0
            && wanted.is_some_and(|band| band != state.band || (slides && band != ScrollBand::WHOLE_SCREEN));
        if must_scroll_whole_screen || must_unturn {
            screen.set_scroll_band(ScrollBand::WHOLE_SCREEN)?;
        }
        Ok(())
    }

    /// The band of the screen's first Scroll or List whose content is taller than it.
    fn scrolling_list_band(&self) -> Option<ScrollBand> {
        (0..self.nodes.len())
            .filter(|&index| clips_children(self.nodes[index].kind) && self.nodes[index].shown)
            .filter(|&index| content_height(&self.nodes, index) > self.nodes[index].bounds.height)
            .find_map(|index| self.scroll_band_of(index))
    }

    /// The rows a Scroll or List shows its content in.
    fn scroll_band_of(&self, node_index: usize) -> Option<ScrollBand> {
        let node = &self.nodes[node_index];
        let area = node.bounds.intersect(node.clip)?.intersect(SCREEN_AREA)?;
        Some(ScrollBand {
            first_row: area.y,
            end_row: area.y + area.height,
        })
    }

    // ------------------------------------------------------------------------
    // Scrolling
    // ------------------------------------------------------------------------

    /// Lays out a list that scrolled by `rows`, and keeps the move for the panel to make if it can.
    pub(super) fn note_scroll(&mut self, node_index: usize, rows: i32) {
        self.measure_and_lay_out();
        let can_wait = !self.whole_screen_dirty
            && self
                .scroll_move
                .is_none_or(|waiting| waiting.node_index == node_index);
        if can_wait {
            let rows_before = self.scroll_move.map_or(0, |waiting| waiting.rows);
            self.scroll_move = Some(ScrollMove {
                node_index,
                rows: rows_before + rows,
            });
            return;
        }
        if let Some(waiting) = self.scroll_move.take() {
            self.mark_subtree_dirty(waiting.node_index);
        }
        self.mark_subtree_dirty(node_index);
    }

    pub(super) fn mark_subtree_dirty(&mut self, node_index: usize) {
        for index in node_index..self.nodes[node_index].subtree_end as usize {
            self.dirty_nodes |= 1 << index;
        }
    }

    /// Draws a scrolled list, by moving its rows in the panel when it can and by drawing them all when not.
    fn draw_scroll<Screen>(
        &mut self,
        screen: &mut Screen,
        strip: &mut [Rgb565],
        scroll_move: ScrollMove,
    ) -> Result<(), Screen::Error>
    where
        Screen: ScrollingScreen,
    {
        if !self.scroll_with_panel(screen, strip, scroll_move)? {
            self.mark_subtree_dirty(scroll_move.node_index);
            return Ok(());
        }
        let inside = scroll_move.node_index + 1..self.nodes[scroll_move.node_index].subtree_end as usize;
        for index in inside {
            let is_dirty = self.dirty_nodes & (1 << index) != 0;
            let node = &mut self.nodes[index];
            node.drawn_bounds = match is_dirty {
                false => node.covered_area(),
                // Its old pixels moved with the panel, so that is where they are cleared from.
                true => node
                    .drawn_bounds
                    .map(|area| Rect {
                        y: area.y - scroll_move.rows,
                        ..area
                    })
                    .and_then(|area| area.intersect(node.clip)),
            };
        }
        Ok(())
    }

    /// Moves a list's rows in the panel and draws only the rows that came into view; false when it cannot.
    fn scroll_with_panel<Screen>(
        &self,
        screen: &mut Screen,
        strip: &mut [Rgb565],
        scroll_move: ScrollMove,
    ) -> Result<bool, Screen::Error>
    where
        Screen: ScrollingScreen,
    {
        let rows = scroll_move.rows;
        if rows == 0 {
            return Ok(true);
        }
        let Some(band) = self.scroll_band_of(scroll_move.node_index) else {
            return Ok(false);
        };
        let is_a_small_move = 2 * rows.abs() < band.height();
        if !screen.can_scroll() || !is_a_small_move || !self.is_alone_in(scroll_move.node_index, band) {
            return Ok(false);
        }
        let state = screen.scroll_state();
        if state.band != band {
            if state.turned_rows != 0 {
                return Ok(false);
            }
            screen.set_scroll_band(band)?;
        }

        // The rows coming into view are drawn over the rows leaving at the other end, which the panel then moves round.
        match rows > 0 {
            true => {
                let incoming = rows_from(band.end_row - rows, rows);
                self.paint(screen, strip, incoming, band.first_row - incoming.y)?;
            }
            false => {
                let incoming = rows_from(band.first_row, -rows);
                self.paint(screen, strip, incoming, band.end_row + rows - band.first_row)?;
            }
        }
        screen.scroll_by(rows)?;
        for edge in self.fixed_edges_in(scroll_move.node_index, band) {
            self.paint(screen, strip, edge, 0)?;
        }
        Ok(true)
    }

    /// Whether nothing but the list and the boxes around it is drawn, or was last drawn, across the band.
    fn is_alone_in(&self, node_index: usize, band: ScrollBand) -> bool {
        let band_area = rows_from(band.first_row, band.height());
        let list = node_index..self.nodes[node_index].subtree_end as usize;
        let mut around = [false; 256];
        let mut next = self.nodes[node_index].parent;
        while let Some(parent) = next {
            around[parent as usize] = true;
            next = self.nodes[parent as usize].parent;
        }
        self.nodes.iter().enumerate().all(|(index, node)| {
            list.contains(&index)
                || around[index]
                || [node.covered_area(), node.drawn_bounds]
                    .into_iter()
                    .flatten()
                    .all(|area| area.intersect(band_area).is_none())
        })
    }

    /// The rows of the band holding the rounded or bordered edges of the list and the boxes around it, which must stay put.
    fn fixed_edges_in(&self, node_index: usize, band: ScrollBand) -> Vec<Rect> {
        let band_area = rows_from(band.first_row, band.height());
        let mut edges = Vec::new();
        let mut next = Some(node_index);
        while let Some(index) = next {
            let node = &self.nodes[index];
            let radius = i32::from(node.settings.radius.unwrap_or(0));
            let border = i32::from(node.settings.border_width.unwrap_or(0));
            let edge_rows = radius.max(border);
            if edge_rows > 0 {
                let top = rows_from(node.bounds.y, edge_rows);
                let bottom = rows_from(node.bounds.y + node.bounds.height - edge_rows, edge_rows);
                edges.extend(
                    [top, bottom]
                        .into_iter()
                        .filter_map(|edge| edge.intersect(band_area)),
                );
            }
            next = node.parent.map(usize::from);
        }
        edges
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
