// The app template, from wasp-os's template: a starting point that answers every kind of input.
// One card shows the app's name and the card below it the last swipe or touch it received. The side
// button leaves, as it does in every app.

import { computed, define_app, on, signal, type ClickEvent, type SwipeEvent } from "neotime";
import { Card } from "neotime/components";
import { icons } from "neotime/icons";
import { Label, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const APP_NAME = "Template";

/** What the last event was: none, a swipe or a touch. */
const last_event_kind = signal<"none" | "swipe" | "touch">("none");

/** The last swipe's direction, as a place in `DIRECTION_NAMES`. */
const last_direction = signal(0);

const DIRECTION_NAMES = ["up", "down", "left", "right"];

/** Where the last touch landed. */
const last_x = signal(0);
const last_y = signal(0);

// ============================================================================
// Derived values
// ============================================================================

const last_event_text = computed(() =>
  last_event_kind.value === "swipe"
    ? `Swipe ${DIRECTION_NAMES[last_direction.value]}`
    : last_event_kind.value === "touch"
      ? `(${last_x.value}, ${last_y.value})`
      : "",
);

// ============================================================================
// Handlers
// ============================================================================

function swipe(event: SwipeEvent) {
  last_event_kind.value = "swipe";
  last_direction.value =
    event.direction === "up" ? 0 : event.direction === "down" ? 1 : event.direction === "left" ? 2 : 3;
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
    <Card grow={1} justify="center" padding={12}>
      <Label text_align="center">{props.text}</Label>
    </Card>
  );
}

function Template() {
  return (
    <Screen onSwipe={swipe} onClick={touch} padding={8} gap={6}>
      <TextCard text={APP_NAME} />
      <TextCard text={last_event_text.value} />
    </Screen>
  );
}

on("open", () => {
  last_event_kind.value = "none";
});

export default define_app({
  id: "template",
  name: APP_NAME,
  group: "tools",
  icon: icons.settings,
  screens: { main: Template },
});
