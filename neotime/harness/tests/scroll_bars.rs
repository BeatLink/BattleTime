//! Scroll bars and the ways a list scrolls: down or across, smoothly or a page at a time.

use neotime_app_testing::{Direction, EffectKind, TestWatch};

// ============================================================================
// Apps to scroll
// ============================================================================

/// A full-screen Scroll of ten tall labels, with the scroll props given.
fn tall_scroll(props: &str) -> TestWatch {
    let source = format!(
        r#"
import {{ define_app, range }} from "neotime";
import {{ Label, Screen, Scroll }} from "neotime/widgets";

function Lines() {{
  return (
    <Screen>
      <Scroll grow={{1}} {props}>
        {{range(10).map((place) => (
          <Label height={{60}}>{{`Line ${{place}}`}}</Label>
        ))}}
      </Scroll>
    </Screen>
  );
}}

export default define_app({{ id: "lines", name: "Lines", screens: {{ main: Lines }} }});
"#
    );
    TestWatch::open_source(&source).unwrap()
}

/// A full-screen List of twelve cards side by side, each `width` wide, with the scroll props given.
fn wide_list(width: i32, props: &str) -> TestWatch {
    let source = format!(
        r#"
import {{ define_app, range }} from "neotime";
import {{ Button, List, Screen }} from "neotime/widgets";

function Cards() {{
  return (
    <Screen>
      <List grow={{1}} direction="row" {props}>
        {{range(12).map((place) => (
          <Button width={{{width}}}>{{`Card ${{place}}`}}</Button>
        ))}}
      </List>
    </Screen>
  );
}}

export default define_app({{ id: "cards", name: "Cards", screens: {{ main: Cards }} }});
"#
    );
    TestWatch::open_source(&source).unwrap()
}

fn left_of(watch: &TestWatch, text: &str) -> i32 {
    watch
        .area_of_text(text)
        .unwrap_or_else(|| panic!("{text} is not shown"))
        .x
}

// ============================================================================
// The bar
// ============================================================================

#[test]
fn a_scroll_longer_than_the_screen_draws_a_bar_down_its_right_edge_and_keeps_room_for_it() {
    let mut with_bar = tall_scroll("");
    let mut without_bar = tall_scroll("scroll_bar={false}");

    let frame = with_bar.fresh_frame();
    let background = frame.pixel(120, 239);
    let thumb = frame.pixel(239, 10);
    let track = frame.pixel(239, 200);
    assert_ne!(thumb, background);
    assert_ne!(track, background);
    assert_ne!(thumb, track);
    assert_eq!(frame.pixel(239, 0), background, "the bar leaves its ends clear");
    assert_eq!(without_bar.fresh_frame().pixel(239, 10), background);

    let room =
        without_bar.area_of_text("Line 0").unwrap().width - with_bar.area_of_text("Line 0").unwrap().width;
    assert_eq!(room, 4);
}

#[test]
fn a_scroll_that_fits_draws_no_bar() {
    let source = r#"
import { define_app } from "neotime";
import { Label, Screen, Scroll } from "neotime/widgets";

function Short() {
  return (
    <Screen>
      <Scroll grow={1}>
        <Label>Only line</Label>
      </Scroll>
    </Screen>
  );
}

export default define_app({ id: "short", name: "Short", screens: { main: Short } });
"#;
    let mut watch = TestWatch::open_source(source).unwrap();
    let frame = watch.fresh_frame();
    assert_eq!(frame.pixel(239, 10), frame.pixel(120, 239));
}

#[test]
fn the_thumb_moves_down_the_bar_as_the_scroll_goes_down() {
    let mut watch = tall_scroll("");
    let top_frame = watch.fresh_frame();
    let thumb = top_frame.pixel(239, 10);

    watch.drag((120, 220), (120, 20));
    watch.drag((120, 220), (120, 20));
    assert_eq!(watch.pixels_left_stale(), 0);
    let frame = watch.fresh_frame();

    assert_ne!(frame.pixel(239, 10), thumb);
    assert_eq!(frame.pixel(239, 230), thumb);
    assert!(watch.shows_text("Line 9"), "{:?}", watch.visible_texts());
}

#[test]
fn a_paged_bar_has_a_segment_per_page_with_the_page_on_show_bright() {
    let mut watch = tall_scroll(r#"scrolling="pages""#);
    let frame = watch.fresh_frame();
    let background = frame.pixel(120, 239);

    // 600 rows of content in a 240-row view make three pages, 80 rows of bar each.
    let first = frame.pixel(239, 40);
    let second = frame.pixel(239, 120);
    assert_ne!(first, second);
    assert_eq!(frame.pixel(239, 200), second);
    assert_eq!(frame.pixel(239, 80), background, "the segments stand apart");
}

// ============================================================================
// Pages
// ============================================================================

#[test]
fn a_paged_scroll_turns_a_whole_page_on_a_short_drag_and_stays_on_a_shorter_one() {
    let mut watch = tall_scroll(r#"scrolling="pages""#);

    watch.drag((120, 200), (120, 170));
    assert_eq!(watch.area_of_text("Line 4").unwrap().y, 0);

    watch.drag((120, 100), (120, 90));
    assert_eq!(watch.area_of_text("Line 4").unwrap().y, 0);

    watch.drag((120, 200), (120, 100));
    assert!(watch.shows_text("Line 9"), "{:?}", watch.visible_texts());
    assert_eq!(
        watch.area_of_text("Line 9").unwrap().y,
        180,
        "the last page ends with the content"
    );

    watch.drag((120, 100), (120, 130));
    assert_eq!(watch.area_of_text("Line 4").unwrap().y, 0);
    assert_eq!(watch.pixels_left_stale(), 0);
}

// ============================================================================
// Across
// ============================================================================

#[test]
fn a_list_running_across_scrolls_left_and_right_and_builds_only_the_cards_in_view() {
    let mut watch = wide_list(100, "");
    assert_eq!(left_of(&watch, "Card 0"), 0);
    let nodes_at_the_start = watch.host().node_count();

    watch.drag((220, 100), (20, 100));
    watch.drag((220, 100), (20, 100));
    watch.drag((220, 100), (20, 100));

    assert!(!watch.shows_text("Card 0"));
    assert!(watch.shows_text("Card 6"), "{:?}", watch.visible_texts());
    assert!(watch.host().node_count() <= nodes_at_the_start + 1);
    assert_eq!(watch.pixels_left_stale(), 0);
}

#[test]
fn a_list_running_across_draws_its_bar_along_the_bottom() {
    let mut watch = wide_list(100, "");
    let frame = watch.fresh_frame();
    let background = frame.pixel(239, 10);
    assert_ne!(frame.pixel(10, 239), background);
    assert_eq!(frame.pixel(239, 10), background);
}

#[test]
fn a_paged_list_running_across_turns_a_screen_at_a_time() {
    let mut watch = wide_list(240, r#"scrolling="pages""#);

    watch.drag((200, 100), (160, 100));
    assert_eq!(left_of(&watch, "Card 1"), 0);

    watch.drag((40, 100), (80, 100));
    assert_eq!(left_of(&watch, "Card 0"), 0);
}

#[test]
fn a_swipe_that_scrolled_a_list_across_does_not_also_go_back() {
    let mut watch = wide_list(100, "");
    watch.drag((220, 100), (20, 100));
    let before = left_of(&watch, "Card 3");
    watch.take_effects();

    watch.swipe(Direction::Right);

    assert!(watch.effects_of(EffectKind::Back).is_empty());
    assert!(left_of(&watch, "Card 3") > before);
}
