// Fibonacci clock watch face, ported from wasp-os's fibonacci_clock.
// Five squares of sides 1, 1, 2, 3 and 5: red ones add up to the hour, green ones to the minutes in fives,
// blue ones count for both, and white ones for neither.

import {
  battery_percent,
  computed,
  define_app,
  format_time,
  hour_of,
  minute_of,
  now,
  type Color,
} from "battletime";
import { StatusIcons } from "battletime/components";
import { icons } from "battletime/icons";
import { Column, Label, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

/** White for neither, red for hours, green for minutes and blue for both. */
const SQUARE_COLORS: Color[] = ["#ffffff", "#ff0000", "#00ff00", "#0000ff"];

/**
 * How each number from 0 to 12 splits into the squares 5, 3, 2, 1 and 1, largest first,
 * as a mask with 1 for the 5, 2 for the 3, 4 for the 2, 8 for the first 1 and 16 for the second.
 */
const FIBONACCI_MASKS = [0, 8, 4, 2, 10, 1, 9, 5, 3, 11, 7, 15, 31];

// ============================================================================
// Derived values
// ============================================================================

const hour_mask = computed(() => FIBONACCI_MASKS[hour_of(now.value) % 12]);

/** The clock shows minutes in steps of five. */
const minute_mask = computed(() => FIBONACCI_MASKS[Math.floor(minute_of(now.value) / 5)]);

/** Green when well charged, then yellow, then red, as wasp-os's battery meter. */
const battery_color = computed<Color>(() =>
  battery_percent.value > 50 ? "#00c000" : battery_percent.value > 20 ? "#e0c000" : "#e00000",
);

// ============================================================================
// Screen
// ============================================================================

function StatusBar() {
  return (
    <StatusIcons
      x={0}
      y={0}
      width={240}
      height={32}
      align="end"
      gap={6}
      padding={4}
      icon_size={22}
      color="accent"
      battery_color={battery_color.value}
    />
  );
}

/** One square, coloured by whether the hour and the minutes use it; `weight` is its bit in the masks. */
function Square(props: { x: number; y: number; size: number; weight: number }) {
  const uses_for_hour = Math.floor(hour_mask.value / props.weight) % 2;
  const uses_for_minutes = Math.floor(minute_mask.value / props.weight) % 2;
  return (
    <Column
      x={props.x}
      y={props.y}
      width={props.size}
      height={props.size}
      background={SQUARE_COLORS[uses_for_hour + 2 * uses_for_minutes]}
    />
  );
}

function FibonacciClockFace() {
  return (
    <Screen>
      <StatusBar />
      <Square x={71} y={60} size={23} weight={16} />
      <Square x={71} y={85} size={23} weight={8} />
      <Square x={21} y={60} size={48} weight={4} />
      <Square x={21} y={110} size={73} weight={2} />
      <Square x={96} y={60} size={123} weight={1} />
      <Label x={0} y={202} width={240} text_align="center">
        {format_time(now.value, "D MMM YYYY")}
      </Label>
    </Screen>
  );
}

export default define_app({
  id: "fibonacci-clock",
  name: "Fibonacci clock",
  icon: icons.clock,
  kind: "face",
  screens: { main: FibonacciClockFace },
});
