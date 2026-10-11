// The app template, from wasp-os's template: a starting point that answers every kind of input.
// One card shows the app's name and the card below it the last event it received. Swipe right or hold
// the button to leave.

import { signal } from "@preact/signals";
import { back, define_app, type ButtonEvent, type ClickEvent, type SwipeEvent } from "neotime";
import { icons } from "neotime/icons";
import { Column, Label, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const APP_NAME = "Template";

const last_event = signal("");

// ============================================================================
// Handlers
// ============================================================================

function button(event: ButtonEvent) {
  if (event.kind === "long_press") back();
  else last_event.value = "Button";
}

function swipe(event: SwipeEvent) {
  if (event.direction === "right") back();
  else last_event.value = `Swipe ${event.direction}`;
}

function touch(event: ClickEvent) {
  last_event.value = `(${event.x}, ${event.y})`;
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
    <Screen onButton={button} onSwipe={swipe} onClick={touch} padding={6} gap={6}>
      <TextCard text={APP_NAME} />
      <TextCard text={last_event.value} />
    </Screen>
  );
}

export default define_app({
  id: "template",
  name: APP_NAME,
  icon: icons.settings,
  screens: { main: Template },
  onOpen: () => {
    last_event.value = "";
  },
});
