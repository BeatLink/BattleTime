// Dice, ported from InfiniTime's Dice.
// Roll one to nine dice of two to ninety-nine sides with the button or by shaking the watch.
// A single two-sided die shows heads or tails, and each roll changes colour so a repeat is noticed.

import { computed, signal } from "@preact/signals";
import { define_app, format_number, motion, now, persisted, random_int, vibrate, SECOND, type Instant } from "neotime";
import { NumberPicker } from "neotime/components";
import { icons } from "neotime/icons";
import { Button, Column, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const DIE_PLACES = [0, 1, 2, 3, 4, 5, 6, 7, 8];

/** The colours a roll cycles through: yellow, magenta and aqua. */
const RESULT_COLORS: `#${string}`[] = ["#ffff00", "#ff00ff", "#00ffff"];

/** The shortest gap between two rolls by shaking. */
const SHAKE_PAUSE = SECOND;

const die_count = persisted("die_count", 1);
const side_count = persisted("side_count", 6);

/** The faces of the last roll; only the first `rolled_count` count. */
const faces = signal<number[]>([0, 0, 0, 0, 0, 0, 0, 0, 0]);
const rolled_count = signal(1);
const rolled_sides = signal(6);
const total = signal(0);
const color_place = signal(0);
const last_rolled_at = signal<Instant>(0);
const is_open = signal(false);

// ============================================================================
// Derived values
// ============================================================================

function face_text(place: number, count: number, values: number[]): string {
  return place < count ? `${place > 0 ? "+" : ""}${format_number(values[place])}` : "";
}

const breakdown = computed(() =>
  rolled_count.value === 1
    ? rolled_sides.value === 2
      ? faces.value[0] === 1
        ? "HEADS"
        : "TAILS"
      : ""
    : `${face_text(0, rolled_count.value, faces.value)}${face_text(1, rolled_count.value, faces.value)}${face_text(2, rolled_count.value, faces.value)}${face_text(3, rolled_count.value, faces.value)}${face_text(4, rolled_count.value, faces.value)}${face_text(5, rolled_count.value, faces.value)}${face_text(6, rolled_count.value, faces.value)}${face_text(7, rolled_count.value, faces.value)}${face_text(8, rolled_count.value, faces.value)}`,
);

const result_color = computed(() => RESULT_COLORS[color_place.value]);

// ============================================================================
// Handlers
// ============================================================================

function throw_dice() {
  const thrown = DIE_PLACES.map(() => random_int(1, side_count.value));
  let sum = 0;
  for (const place of DIE_PLACES) {
    if (place < die_count.value) sum = sum + thrown[place];
  }
  faces.value = thrown;
  total.value = sum;
  rolled_count.value = die_count.value;
  rolled_sides.value = side_count.value;
  last_rolled_at.value = now.value;
}

function roll() {
  throw_dice();
  color_place.value = (color_place.value + 1) % RESULT_COLORS.length;
  vibrate(30);
}

function roll_on_shake() {
  if (is_open.value && now.value - last_rolled_at.value >= SHAKE_PAUSE) roll();
}

function open() {
  is_open.value = true;
  throw_dice();
  color_place.value = random_int(0, RESULT_COLORS.length - 1);
}

// ============================================================================
// Screen
// ============================================================================

function Dice() {
  return (
    <Screen padding={4} gap={4}>
      <Row grow={1} gap={4} align_items="start">
        <Column align_items="center">
          <Label font="small" color="muted">count</Label>
          <NumberPicker value={die_count} minimum={1} maximum={9} />
        </Column>
        <Column align_items="center">
          <Label font="small" color="muted">sides</Label>
          <NumberPicker value={side_count} minimum={2} maximum={99} digits={2} />
        </Column>
        <Column grow={1} align_items="center" gap={6} padding={4}>
          <Label font="large" color={result_color.value}>{format_number(total.value)}</Label>
          <Label font="small" color={result_color.value} text_align="center">
            {breakdown.value}
          </Label>
        </Column>
      </Row>
      <Button height={52} onClick={roll}>
        <Icon icon={icons.dice} />
      </Button>
    </Screen>
  );
}

export default define_app({
  id: "dice",
  name: "Dice",
  icon: icons.dice,
  screens: { main: Dice },
  subscriptions: [motion("shake", roll_on_shake)],
  permissions: ["motion"],
  onOpen: open,
  onClose: () => {
    is_open.value = false;
  },
});
