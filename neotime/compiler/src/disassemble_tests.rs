//! Tests for the image listing, from images the compiler builds and from bytes that are not one.

use super::disassemble;
use crate::compile_source;

const SOURCE: &str = r#"
import { define_app, format_number, now, on, persisted, signal, vibrate } from "neotime";
import { Button, Label, Screen } from "neotime/widgets";

const count = persisted("count", 2);
const cells = signal([0, 0, 0]);
function roll() {
  for (const place of [0, 1, 2]) cells.value[place] = count.value;
  count.value = count.value > 3 ? 0 : count.value + 1;
  vibrate(30);
}
function Main() {
  return <Screen><Label>{`Rolled ${format_number(count.value)}`}</Label><Button onClick={roll}>Roll</Button></Screen>;
}
on("open", roll);
export default define_app({ id: "test", name: "Test", screens: { main: Main } });
"#;

fn listing() -> String {
    let output = compile_source(SOURCE);
    let errors: Vec<String> = output.errors().map(|error| error.message.clone()).collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    disassemble(&output.image.expect("an image")).expect("a listing")
}

#[test]
fn the_state_table_names_each_variable_with_its_shape() {
    let text = listing();

    assert!(text.contains("count"), "{text}");
    assert!(text.contains(", persists"), "{text}");
    assert!(text.contains("cells") && text.contains("[3] x"), "{text}");
}

#[test]
fn subscriptions_and_every_code_block_are_listed() {
    let text = listing();

    assert!(
        text.contains("subscriptions:\n   0 Subscription { kind: Open"),
        "{text}"
    );
    assert!(text.contains("expression 0:"), "{text}");
    assert!(text.contains("handler 0:"), "{text}");
}

#[test]
fn operands_are_named_and_jumps_show_their_targets() {
    let text = listing();

    assert!(text.contains("WriteState(0)  ; count"), "{text}");
    assert!(
        text.contains("PushText(") && text.contains("; \"Rolled \""),
        "{text}"
    );
    assert!(text.contains("JumpIfFalse(") && text.contains("; to "), "{text}");
    assert!(text.contains("Effect(Vibrate)"), "{text}");
}

#[test]
fn bytes_that_are_not_an_image_are_refused() {
    let problem = disassemble(b"not an image").unwrap_err();

    assert!(problem.starts_with("not an app image"), "{problem}");
}
