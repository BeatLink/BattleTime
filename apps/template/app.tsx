// The app template, from wasp-os's template: a starting point that answers every kind of input.
// One card shows the app's name and the card below it the last event it received. Swipe right or hold
// the button to leave.

import { computed, signal } from "@preact/signals";
import { back, define_app, type ButtonEvent, type ClickEvent, type SwipeEvent } from "neotime";
import { icons } from "neotime/icons";
import { Column, Label, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const APP_NAME = "Template";

/** What the last event was: none, the button, a swipe or a touch. */
const last_event_kind = signal<"none" | "button" | "swipe" | "touch">("none");

/** The last swipe's direction, as a place in `DIRECTION_NAMES`. */
const last_direction = signal(0);

const DIRECTION_NAMES = ["up", "down", "left"];

/** Where the last touch landed. */
const last_x = signal(0);
const last_y = signal(0);

// ============================================================================
// Derived values
// ============================================================================

const last_event_text = computed(() =>
  last_event_kind.value === "button"
    ? "Button"
    : last_event_kind.value === "swipe"
      ? `Swipe ${DIRECTION_NAMES[last_direction.value]}`
      : last_event_kind.value === "touch"
        ? `(${last_x.value}, ${last_y.value})`
        : "",
);

// ============================================================================
// Handlers
// ============================================================================

function button(event: ButtonEvent) {
  if (event.kind === "long_press") back();
  else last_event_kind.value = "button";
}

function swipe(event: SwipeEvent) {
  if (event.direction === "right") back();
  else {
    last_event_kind.value = "swipe";
    last_direction.value = event.direction === "up" ? 0 : event.direction === "down" ? 1 : 2;
  }
}

function touch(event: ClickEvent) {
  last_event_kind.value = "touch";
  last_x.value = event.x;
  last_y.value = event.y;
}

// ============================================================================
// Screen
// ============================================================================

function TextCard(props: { text: string }) {
  return (
    <Column grow={1} justify="center" background="surface" radius={12} padding={12}>
      <Label text_align="center">{props.text}</Label>
    </Column>
  );
}

function Template() {
  return (
    <Screen onButton={button} onSwipe={swipe} onClick={touch} padding={8} gap={6}>
      <TextCard text={APP_NAME} />
      <TextCard text={last_event_text.value} />
    </Screen>
  );
}

export default define_app({
  id: "template",
  name: APP_NAME,
  icon: icons.settings,
  screens: { main: Template },
  onOpen: () => {
    last_event_kind.value = "none";
  },
});
