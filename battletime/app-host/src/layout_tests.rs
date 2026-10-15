// The host keeps each node in its own allocation, and these tests build nodes the same way.
#![allow(clippy::vec_box)]

use alloc::vec::Vec;

use super::*;
use crate::fonts::FontName;
use crate::nodes::OptionalPixels;
use crate::text::wrapped_line_count;

// ============================================================================
// Helpers
// ============================================================================

const SCREEN: Rect = Rect {
    x: 0,
    y: 0,
    width: 240,
    height: 240,
};

/// Nodes in tree order, each given with the number of its parent.
fn tree(shape: &[(WidgetKind, Option<u8>)]) -> Vec<Box<Node>> {
    let mut nodes: Vec<Box<Node>> = shape
        .iter()
        .enumerate()
        .map(|(index, &(kind, parent))| {
            Box::new(Node::new(index as u16, kind, index as u8 + 1, parent, SCREEN))
        })
        .collect();
    for index in (0..nodes.len()).rev() {
        if let Some(parent) = nodes[index].parent {
            let end = nodes[index].subtree_end;
            let parent_node = &mut nodes[parent as usize];
            parent_node.subtree_end = parent_node.subtree_end.max(end);
        }
    }
    nodes
}

fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect { x, y, width, height }
}

fn laid_out(mut nodes: Vec<Box<Node>>) -> Vec<Box<Node>> {
    lay_out(&mut nodes, SCREEN);
    nodes
}

fn normal_line_height() -> i32 {
    line_height(FontName::Normal.metrics())
}

fn normal_text_width(text: &str) -> i32 {
    text_width(FontName::Normal.metrics(), text)
}

// ============================================================================
// Arcs
// ============================================================================

#[test]
fn an_arc_stacks_its_children_in_its_middle() {
    let nodes = tree(&[
        (WidgetKind::Arc, None),
        (WidgetKind::Led, Some(0)),
        (WidgetKind::Led, Some(0)),
    ]);

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(112, 104, 16, 16));
    assert_eq!(nodes[2].bounds(), rect(112, 120, 16, 16));
    assert!(nodes[1].shown && nodes[2].shown);
}

#[test]
fn a_click_area_stacks_its_children_and_takes_their_size() {
    let nodes = tree(&[
        (WidgetKind::Arc, None),
        (WidgetKind::ClickArea, Some(0)),
        (WidgetKind::Led, Some(1)),
        (WidgetKind::Led, Some(1)),
    ]);

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(112, 104, 16, 32));
    assert_eq!(nodes[2].bounds(), rect(112, 104, 16, 16));
    assert_eq!(nodes[3].bounds(), rect(112, 120, 16, 16));
}

// ============================================================================
// Rows
// ============================================================================

#[test]
fn a_row_lays_its_children_left_to_right_gap_apart() {
    let mut nodes = tree(&[
        (WidgetKind::Row, None),
        (WidgetKind::Led, Some(0)),
        (WidgetKind::Led, Some(0)),
        (WidgetKind::Switch, Some(0)),
    ]);
    nodes[0].settings.gap = 4;

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(0, 0, 16, 240));
    assert_eq!(nodes[2].bounds(), rect(20, 0, 16, 240));
    assert_eq!(nodes[3].bounds(), rect(40, 0, 52, 240));
}

#[test]
fn align_items_places_row_children_across_at_their_natural_height() {
    let expected = [
        (Placement::Start, 0),
        (Placement::Center, 112),
        (Placement::End, 224),
    ];

    for (align_items, expected_y) in expected {
        let mut nodes = tree(&[(WidgetKind::Row, None), (WidgetKind::Led, Some(0))]);
        nodes[0].settings.align_items = Some(align_items);

        let nodes = laid_out(nodes);

        assert_eq!(nodes[1].bounds(), rect(0, expected_y, 16, 16), "{align_items:?}");
    }
}

#[test]
fn justify_places_row_children_along_the_spare_space() {
    let expected = [
        (Placement::Start, [0, 16]),
        (Placement::Center, [104, 120]),
        (Placement::End, [208, 224]),
        (Placement::SpaceBetween, [0, 224]),
    ];

    for (justify, expected_x) in expected {
        let mut nodes = tree(&[
            (WidgetKind::Row, None),
            (WidgetKind::Led, Some(0)),
            (WidgetKind::Led, Some(0)),
        ]);
        nodes[0].settings.justify = justify;

        let nodes = laid_out(nodes);

        assert_eq!(
            [nodes[1].bounds().x, nodes[2].bounds().x],
            expected_x,
            "{justify:?}"
        );
    }
}

#[test]
fn spare_row_space_goes_to_growing_children_in_proportion() {
    let mut nodes = tree(&[
        (WidgetKind::Row, None),
        (WidgetKind::Led, Some(0)),
        (WidgetKind::Led, Some(0)),
    ]);
    nodes[0].settings.justify = Placement::End;
    nodes[1].settings.grow = 1;
    nodes[2].settings.grow = 3;

    let nodes = laid_out(nodes);

    assert_eq!((nodes[1].bounds().x, nodes[1].bounds().width), (0, 16 + 52));
    assert_eq!((nodes[2].bounds().x, nodes[2].bounds().width), (68, 16 + 156));
}

#[test]
fn a_row_inside_a_column_is_as_wide_as_its_children_and_gaps() {
    let mut nodes = tree(&[
        (WidgetKind::Column, None),
        (WidgetKind::Row, Some(0)),
        (WidgetKind::Led, Some(1)),
        (WidgetKind::Switch, Some(1)),
    ]);
    nodes[0].settings.align_items = Some(Placement::Start);
    nodes[1].settings.gap = 2;

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(0, 0, 70, 28));
    assert_eq!(nodes[3].bounds(), rect(18, 0, 52, 28));
}

// ============================================================================
// Columns
// ============================================================================

#[test]
fn a_column_child_fills_the_width_inside_padding_and_margin() {
    let mut nodes = tree(&[(WidgetKind::Column, None), (WidgetKind::Bar, Some(0))]);
    nodes[0].settings.padding = 10;
    nodes[1].settings.margin = 5;

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(15, 15, 210, 10));
}

#[test]
fn a_column_child_with_a_percent_width_is_centred() {
    let mut nodes = tree(&[(WidgetKind::Column, None), (WidgetKind::Bar, Some(0))]);
    nodes[1].settings.width = Some(Length::Percent(50));

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(60, 0, 120, 10));
}

#[test]
fn fixed_widgets_take_their_own_size_when_aligned() {
    let kinds_and_sizes = [
        (WidgetKind::Arc, 100, 100),
        (WidgetKind::Bar, 120, 10),
        (WidgetKind::Led, 16, 16),
        (WidgetKind::Switch, 52, 28),
        (WidgetKind::Slider, 120, 16),
        (WidgetKind::Chart, 200, 100),
    ];

    for (kind, width, height) in kinds_and_sizes {
        let mut nodes = tree(&[(WidgetKind::Column, None), (kind, Some(0))]);
        nodes[0].settings.align_items = Some(Placement::Start);

        let nodes = laid_out(nodes);

        assert_eq!(nodes[1].bounds(), rect(0, 0, width, height), "{kind:?}");
    }
}

#[test]
fn a_button_is_as_wide_as_its_icon_and_text_with_side_space() {
    let mut nodes = tree(&[
        (WidgetKind::Column, None),
        (WidgetKind::Button, Some(0)),
        (WidgetKind::Button, Some(0)),
    ]);
    nodes[0].settings.align_items = Some(Placement::Start);
    for button in &mut nodes[1..=2] {
        button.content_width = 20;
        button.content_height = 20;
    }
    nodes[2].text.push_str("OK");

    let nodes = laid_out(nodes);

    assert_eq!(
        nodes[1].bounds(),
        rect(0, 0, 20 + 32, normal_line_height().max(44))
    );
    assert_eq!(
        nodes[2].bounds().width,
        normal_text_width("OK") + 20 + TICK_BOX_GAP + 32
    );
    assert_eq!(nodes[2].bounds().height, normal_line_height().max(44));
}

#[test]
fn an_icon_in_a_button_keeps_its_own_size_in_the_middle() {
    let mut nodes = tree(&[
        (WidgetKind::Button, None),
        (WidgetKind::Icon, Some(0)),
        (WidgetKind::Image, Some(0)),
    ]);
    nodes[0].settings.width = Some(Length::Pixels(68));
    nodes[0].settings.height = Some(Length::Pixels(64));
    for child in &mut nodes[1..=2] {
        child.content_width = 32;
        child.content_height = 32;
    }

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(18, 16, 32, 32));
    assert_eq!(nodes[2].bounds(), rect(0, 0, 68, 64));
}

#[test]
fn a_checkbox_is_its_tick_box_then_its_text() {
    let mut nodes = tree(&[(WidgetKind::Column, None), (WidgetKind::Checkbox, Some(0))]);
    nodes[0].settings.align_items = Some(Placement::Start);
    nodes[1].text.push_str("Alarm");

    let nodes = laid_out(nodes);

    assert_eq!(
        nodes[1].bounds(),
        rect(
            0,
            0,
            TICK_BOX_SIZE + TICK_BOX_GAP + normal_text_width("Alarm"),
            normal_line_height().max(TICK_BOX_SIZE),
        )
    );
}

#[test]
fn a_label_with_a_pixel_width_wraps_inside_it() {
    let text = "The quick brown fox jumps over the lazy dog";
    let mut nodes = tree(&[(WidgetKind::Column, None), (WidgetKind::Label, Some(0))]);
    nodes[0].settings.align_items = Some(Placement::Start);
    nodes[1].settings.width = Some(Length::Pixels(60));
    nodes[1].settings.padding = 4;
    nodes[1].text.push_str(text);
    let lines = wrapped_line_count(text, FontName::Normal, 52);

    let nodes = laid_out(nodes);

    assert!(lines > 1);
    assert_eq!(nodes[1].bounds().width, 60);
    assert_eq!(nodes[1].bounds().height, lines * normal_line_height() + 8);
}

// ============================================================================
// Stacks
// ============================================================================

#[test]
fn stacked_children_fill_unless_sized_or_aligned() {
    let mut nodes = tree(&[
        (WidgetKind::Stack, None),
        (WidgetKind::Led, Some(0)),
        (WidgetKind::Switch, Some(0)),
        (WidgetKind::Bar, Some(0)),
    ]);
    nodes[2].settings.align = Some(Align::End);
    nodes[3].settings.width = Some(Length::Pixels(100));
    nodes[3].settings.height = Some(Length::Pixels(20));

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), SCREEN);
    assert_eq!(nodes[2].bounds(), rect(188, 212, 52, 28));
    assert_eq!(nodes[3].bounds(), rect(70, 110, 100, 20));
}

#[test]
fn a_stack_inside_a_column_is_as_big_as_its_biggest_child() {
    let mut nodes = tree(&[
        (WidgetKind::Column, None),
        (WidgetKind::Stack, Some(0)),
        (WidgetKind::Led, Some(1)),
        (WidgetKind::Switch, Some(1)),
    ]);
    nodes[0].settings.align_items = Some(Placement::Start);

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(0, 0, 52, 28));
}

// ============================================================================
// Grids
// ============================================================================

#[test]
fn a_grid_puts_children_in_rows_that_share_the_spare_height() {
    let mut nodes = tree(&[
        (WidgetKind::Grid, None),
        (WidgetKind::Led, Some(0)),
        (WidgetKind::Led, Some(0)),
        (WidgetKind::Led, Some(0)),
    ]);
    nodes[0].settings.columns = 2;
    nodes[0].settings.gap = 10;

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(0, 0, 115, 115));
    assert_eq!(nodes[2].bounds(), rect(125, 0, 115, 115));
    assert_eq!(nodes[3].bounds(), rect(0, 125, 115, 115));
}

#[test]
fn an_aligned_grid_cell_child_keeps_its_natural_size() {
    let mut nodes = tree(&[(WidgetKind::Grid, None), (WidgetKind::Led, Some(0))]);
    nodes[1].settings.align = Some(Align::Start);

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(0, 0, 16, 16));
}

#[test]
fn a_grid_inside_a_column_is_as_big_as_its_cells_and_gaps() {
    let mut nodes = tree(&[
        (WidgetKind::Column, None),
        (WidgetKind::Grid, Some(0)),
        (WidgetKind::Led, Some(1)),
        (WidgetKind::Led, Some(1)),
        (WidgetKind::Led, Some(1)),
    ]);
    nodes[0].settings.align_items = Some(Placement::Start);
    nodes[1].settings.columns = 2;
    nodes[1].settings.gap = 10;

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(0, 0, 42, 42));
    assert_eq!(nodes[3].bounds(), rect(26, 0, 16, 16));
    assert_eq!(nodes[4].bounds(), rect(0, 26, 16, 16));
}

#[test]
fn a_grid_without_columns_has_one_column() {
    let nodes = tree(&[
        (WidgetKind::Grid, None),
        (WidgetKind::Led, Some(0)),
        (WidgetKind::Led, Some(0)),
    ]);

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(0, 0, 240, 120));
    assert_eq!(nodes[2].bounds(), rect(0, 120, 240, 120));
}

#[test]
fn an_empty_grid_still_fills_the_screen() {
    let nodes = laid_out(tree(&[(WidgetKind::Grid, None)]));

    assert_eq!(nodes[0].bounds(), SCREEN);
    assert!(nodes[0].shown);
}

// ============================================================================
// Scrolling
// ============================================================================

#[test]
fn scrolling_moves_the_children_up_and_they_are_clipped_to_the_scroll() {
    let mut nodes = tree(&[
        (WidgetKind::Column, None),
        (WidgetKind::Scroll, Some(0)),
        (WidgetKind::Chart, Some(1)),
        (WidgetKind::Chart, Some(1)),
    ]);
    nodes[0].settings.padding = 20;
    nodes[1].settings.height = Some(Length::Pixels(150));
    nodes[1].scroll_offset = 50;

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(20, 20, 200, 150));
    assert_eq!(nodes[1].clip(), SCREEN);
    assert_eq!(nodes[2].bounds().y, 20 - 50);
    assert_eq!(nodes[3].bounds().y, 20 - 50 + 100);
    assert_eq!(nodes[2].clip(), rect(20, 20, 200, 150));
}

#[test]
fn a_growing_scroll_fills_the_space_left_however_tall_its_content() {
    let mut nodes = tree(&[
        (WidgetKind::Column, None),
        (WidgetKind::Chart, Some(0)),
        (WidgetKind::Scroll, Some(0)),
        (WidgetKind::Chart, Some(2)),
        (WidgetKind::Chart, Some(2)),
        (WidgetKind::Chart, Some(2)),
    ]);
    nodes[1].settings.height = Some(Length::Pixels(40));
    nodes[2].settings.grow = 1;

    let nodes = laid_out(nodes);

    assert_eq!(nodes[2].bounds(), rect(0, 40, 240, 200));
    assert_eq!(content_length(&nodes, 2), 300);
}

#[test]
fn a_scroll_wholly_off_the_screen_lets_its_children_draw_nothing() {
    let mut nodes = tree(&[
        (WidgetKind::Column, None),
        (WidgetKind::List, Some(0)),
        (WidgetKind::Led, Some(1)),
    ]);
    nodes[1].settings.y = OptionalPixels::some(300);

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds().y, 300);
    assert_eq!((nodes[2].clip().width, nodes[2].clip().height), (0, 0));
}

#[test]
fn the_content_height_of_a_scroll_adds_children_gaps_margins_and_padding() {
    let mut nodes = tree(&[
        (WidgetKind::Scroll, None),
        (WidgetKind::Chart, Some(0)),
        (WidgetKind::Chart, Some(0)),
        (WidgetKind::Chart, Some(0)),
    ]);
    nodes[0].settings.gap = 10;
    nodes[0].settings.padding = 5;
    nodes[1].settings.margin = 3;
    let mut nodes = laid_out(nodes);
    nodes[3].settings.visible = false;

    let height = content_length(&nodes, 0);

    assert_eq!(height, (100 + 6) + 100 + 10 + 2 * 5);
}

// ============================================================================
// Leaving the flow
// ============================================================================

#[test]
fn a_child_with_a_position_sits_at_that_offset_and_takes_no_flow_space() {
    let mut nodes = tree(&[
        (WidgetKind::Column, None),
        (WidgetKind::Led, Some(0)),
        (WidgetKind::Bar, Some(0)),
    ]);
    nodes[0].settings.padding = 10;
    nodes[1].settings.x = OptionalPixels::some(30);
    nodes[1].settings.y = OptionalPixels::some(40);

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(40, 50, 16, 16));
    assert_eq!(nodes[2].bounds(), rect(10, 10, 220, 10));
}

#[test]
fn a_scale_is_drawn_over_the_whole_of_its_parent() {
    let mut nodes = tree(&[
        (WidgetKind::Column, None),
        (WidgetKind::Scale, Some(0)),
        (WidgetKind::Bar, Some(0)),
    ]);
    nodes[0].settings.padding = 10;

    let nodes = laid_out(nodes);

    assert_eq!(nodes[1].bounds(), rect(10, 10, 220, 220));
    assert_eq!(nodes[2].bounds().y, 10);
}

#[test]
fn a_hidden_child_gives_up_its_place_and_is_not_shown() {
    let mut nodes = tree(&[
        (WidgetKind::Row, None),
        (WidgetKind::Led, Some(0)),
        (WidgetKind::Switch, Some(0)),
    ]);
    nodes[1].settings.visible = false;

    let nodes = laid_out(nodes);

    assert!(!nodes[1].shown);
    assert!(nodes[2].shown);
    assert_eq!(nodes[2].bounds().x, 0);
}

#[test]
fn children_of_a_hidden_container_are_not_shown() {
    let mut nodes = tree(&[
        (WidgetKind::Column, None),
        (WidgetKind::Stack, Some(0)),
        (WidgetKind::Led, Some(1)),
    ]);
    nodes[1].settings.visible = false;

    let nodes = laid_out(nodes);

    assert!(!nodes[1].shown);
    assert!(!nodes[2].shown);
}

// ============================================================================
// The root
// ============================================================================

#[test]
fn laying_out_no_nodes_does_nothing() {
    let mut nodes: Vec<Box<Node>> = Vec::new();

    lay_out(&mut nodes, SCREEN);

    assert!(nodes.is_empty());
}

#[test]
fn a_root_with_a_width_and_position_keeps_them_and_fills_the_height() {
    let mut nodes = tree(&[(WidgetKind::Column, None)]);
    nodes[0].settings.x = OptionalPixels::some(20);
    nodes[0].settings.width = Some(Length::Pixels(100));

    let nodes = laid_out(nodes);

    assert_eq!(nodes[0].bounds(), rect(20, 0, 100, 240));
}

#[test]
fn only_scrolls_and_lists_clip_their_children() {
    assert!(clips_children(WidgetKind::Scroll));
    assert!(clips_children(WidgetKind::List));
    assert!(!clips_children(WidgetKind::Column));
    assert!(!clips_children(WidgetKind::Stack));
}
