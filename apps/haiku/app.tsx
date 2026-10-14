// Haiku viewer, from wasp-os's haiku.
// Each time it opens it shows the next verse, one line to a card, and swiping up leaves, as on wasp-os.
// A tap or a swipe down moves on to the next verse without leaving. The verses are built in.

import { back, computed, define_app, on, persisted, type SwipeEvent } from "neotime";
import { Card } from "neotime/components";
import { icons } from "neotime/icons";
import { Label, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** Every verse as three lines in a row. */
const VERSE_LINES = [
  "Morning on my wrist",
  "a small light wakes before me",
  "and counts every step",

  "Rain against the glass",
  "the watch face glows a soft blue",
  "nobody is late",

  "Seconds fall like leaves",
  "I hold none of them for long",
  "still the hand moves on",

  "One more silent buzz",
  "the kettle forgot again",
  "my wrist remembers",

  "Low battery sign",
  "even the brightest of days",
  "has to rest a while",

  "Swipe up to leave me",
  "the poem will wait right here",
  "patient as the moon",
];

const VERSE_COUNT = 6;

/** The verse shown last, so the next opening moves on. */
const verse = persisted("verse", VERSE_COUNT - 1);

// ============================================================================
// Derived values
// ============================================================================

const first_line = computed(() => VERSE_LINES[verse.value * 3]);
const second_line = computed(() => VERSE_LINES[verse.value * 3 + 1]);
const third_line = computed(() => VERSE_LINES[verse.value * 3 + 2]);

// ============================================================================
// Handlers
// ============================================================================

function next_verse() {
  verse.value = (verse.value + 1) % VERSE_COUNT;
}

function swipe(event: SwipeEvent) {
  if (event.direction === "up" || event.direction === "right") back();
  else if (event.direction === "down") next_verse();
}

// ============================================================================
// Screen
// ============================================================================

function VerseLine(props: { text: string }) {
  return (
    <Card grow={1} justify="center" padding={6}>
      <Label text_align="center">{props.text}</Label>
    </Card>
  );
}

function Haiku() {
  return (
    <Screen onSwipe={swipe} onClick={next_verse} padding={8} gap={6}>
      <VerseLine text={first_line.value} />
      <VerseLine text={second_line.value} />
      <VerseLine text={third_line.value} />
    </Screen>
  );
}

on("open", next_verse);

export default define_app({
  id: "haiku",
  name: "Haiku",
  group: "tools",
  icon: icons.notification,
  screens: { main: Haiku },
});
