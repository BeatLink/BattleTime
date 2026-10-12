//! Tests for lists whose `.map` becomes one repeated row: each compiles an app and reads its widgets.

use neotime_runtime::{AppImage, Property, PropertySource, WidgetKind, WidgetTree};

use crate::compile_source;

fn app(body: &str) -> String {
    format!(
        r#"
import {{ signal }} from "@preact/signals";
import {{ define_app, range, vibrate }} from "neotime";
import {{ icons }} from "neotime/icons";
import {{ Button, Column, Image, Label, List, Row, Scroll, Screen }} from "neotime/widgets";

{body}

export default define_app({{ id: "test", name: "Test", screens: {{ main: Main }} }});
"#
    )
}

fn kinds(source: &str) -> Vec<WidgetKind> {
    let output = compile_source(source);
    let errors: Vec<String> = output
        .errors()
        .map(|error| error.render("test.tsx", source))
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    let bytes = output.image.expect("an image");
    let image = AppImage::parse(&bytes).unwrap();
    let tree = WidgetTree::new(image);
    (0..tree.widget_count())
        .filter_map(|index| tree.widget(index))
        .map(|widget| widget.kind)
        .collect()
}

#[test]
fn a_map_in_a_list_becomes_one_repeated_row() {
    let source = app(r#"
const count = signal(3);
function Main() {
  return <List gap={6}>
    {range(12).map((place) => place < count.value && (
      <Button height={40} onClick={() => vibrate(place)}>{`Row ${place}`}</Button>
    ))}
  </List>;
}
"#);
    let output = compile_source(&source);
    let bytes = output.image.expect("an image");
    let image = AppImage::parse(&bytes).unwrap();
    let tree = WidgetTree::new(image);
    let repeat = tree.widget(1).unwrap();

    assert_eq!(
        kinds(&source),
        [WidgetKind::List, WidgetKind::Repeat, WidgetKind::Button]
    );
    assert_eq!(
        repeat.property(Property::RowCount),
        Some(PropertySource::Constant(12))
    );
    assert_eq!(repeat.property(Property::Gap), Some(PropertySource::Constant(6)));
    assert!(matches!(
        tree.widget(2).unwrap().property(Property::Visible),
        Some(PropertySource::Expression(_))
    ));
}

#[test]
fn a_map_over_an_array_of_records_in_a_scroll_repeats_its_row() {
    let source = app(r#"
const alarms = signal([{ hour: 7, on: true }, { hour: 8, on: false }, { hour: 9, on: true }]);
function Main() {
  return <Screen>
    <Scroll grow={1}>
      {alarms.value.map((alarm) => (
        <Row height={50}><Label>{`${alarm.hour}`}</Label></Row>
      ))}
    </Scroll>
  </Screen>;
}
"#);

    assert_eq!(
        kinds(&source),
        [
            WidgetKind::Column,
            WidgetKind::Scroll,
            WidgetKind::Repeat,
            WidgetKind::Row,
            WidgetKind::Label
        ]
    );
}

#[test]
fn a_map_in_any_widget_that_holds_others_repeats_its_row() {
    let source = app(r#"
function Main() {
  return <Column><Row>{range(3).map((place) => <Label>{`${place}`}</Label>)}</Row></Column>;
}
"#);

    assert_eq!(
        kinds(&source),
        [
            WidgetKind::Column,
            WidgetKind::Row,
            WidgetKind::Repeat,
            WidgetKind::Label
        ]
    );
}

#[test]
fn a_row_of_several_widgets_is_unrolled() {
    let source = app(r#"
function Main() {
  return <List>{range(2).map((place) => <><Label>{`${place}`}</Label><Label>x</Label></>)}</List>;
}
"#);

    assert_eq!(
        kinds(&source)
            .iter()
            .filter(|kind| **kind == WidgetKind::Label)
            .count(),
        4
    );
    assert!(!kinds(&source).contains(&WidgetKind::Repeat));
}

#[test]
fn a_map_over_constants_that_are_not_a_range_is_unrolled() {
    let source = app(r#"
const NAMES = ["a", "b"];
function Main() {
  return <List>{NAMES.map((name) => <Label>{name}</Label>)}</List>;
}
"#);

    assert_eq!(
        kinds(&source),
        [WidgetKind::List, WidgetKind::Label, WidgetKind::Label]
    );
}

#[test]
fn a_row_that_needs_its_element_while_compiling_is_unrolled() {
    let source = app(r#"
function Main() {
  return <List>{range(2).map((place) => <Image src={place === 0 ? icons.timer : icons.dice} />)}</List>;
}
"#);

    assert_eq!(
        kinds(&source),
        [WidgetKind::List, WidgetKind::Image, WidgetKind::Image]
    );
}
