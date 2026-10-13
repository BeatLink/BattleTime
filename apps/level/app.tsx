// Spirit level, ported from wasp-os's level app.
// A bubble moves against the pull of gravity; a tap opens a menu to calibrate on a flat surface or reset.

import { computed, signal } from "@preact/signals";
import { acceleration, back, clamp, define_app, go, persisted, type AccelerationEvent } from "neotime";
import { icons } from "neotime/icons";
import { Button, Column, Label, Line, Screen, Stack } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const CENTER = 120;
const BUBBLE_SIZE = 14;

/** How far the bubble may move from the centre, keeping it inside the card. */
const REACH = 100;

/** Thousandths of g that move the bubble one pixel. */
const READING_PER_PIXEL = 3;

/** Within this many pixels of the centre the watch counts as level. */
const LEVEL_TOLERANCE = 3;

const latest_x = signal(0);
const latest_y = signal(0);

/** The reading on a flat surface, taken away from every reading. */
const calibration_x = persisted("calibration_x", 0);
const calibration_y = persisted("calibration_y", 0);

// ============================================================================
// Derived values
// ============================================================================

/** The bubble goes the other way from gravity, so the reading is negated. */
const bubble_x = computed(() =>
  clamp(Math.trunc((calibration_x.value - latest_x.value) / READING_PER_PIXEL), -REACH, REACH),
);

const bubble_y = computed(() =>
  clamp(Math.trunc((calibration_y.value - latest_y.value) / READING_PER_PIXEL), -REACH, REACH),
);

const is_level = computed(
  () => Math.abs(bubble_x.value) <= LEVEL_TOLERANCE && Math.abs(bubble_y.value) <= LEVEL_TOLERANCE,
);

// ============================================================================
// Handlers
// ============================================================================

function record(event: AccelerationEvent) {
  latest_x.value = event.x;
  latest_y.value = event.y;
}

function calibrate() {
  calibration_x.value = latest_x.value;
  calibration_y.value = latest_y.value;
  back();
}

function reset_calibration() {
  calibration_x.value = 0;
  calibration_y.value = 0;
  back();
}

// ============================================================================
// Screens
// ============================================================================

function Level() {
  return (
    <Screen onClick={() => go("menu")}>
      <Stack x={4} y={4} width={232} height={232} background="surface" radius={16} />
      <Line points={[{ x: 20, y: CENTER }, { x: 220, y: CENTER }]} color="muted" thickness={1} />
      <Line points={[{ x: CENTER, y: 20 }, { x: CENTER, y: 220 }]} color="muted" thickness={1} />
      <Stack
        x={CENTER + bubble_x.value - Math.floor(BUBBLE_SIZE / 2)}
        y={CENTER + bubble_y.value - Math.floor(BUBBLE_SIZE / 2)}
        width={BUBBLE_SIZE}
        height={BUBBLE_SIZE}
        radius={Math.floor(BUBBLE_SIZE / 2)}
        background={is_level.value ? "success" : "text"}
      />
    </Screen>
  );
}

function Menu() {
  return (
    <Screen padding={8} gap={6}>
      <Button grow={1} onClick={calibrate}>
        Calibrate
      </Button>
      <Button grow={1} onClick={reset_calibration}>
        Reset
      </Button>
      <Button grow={1} onClick={back}>
        Cancel
      </Button>
      <Column align_items="center">
        <Label font="small" color="muted">Calibrate on a flat surface</Label>
      </Column>
    </Screen>
  );
}

export default define_app({
  id: "level",
  name: "Level",
  group: "tools",
  icon: icons.level,
  screens: { main: Level, menu: Menu },
  subscriptions: [acceleration({ milliseconds: 125 }, record, { only_while_open: true })],
  permissions: ["motion"],
});
