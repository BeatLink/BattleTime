// Fibonacci clock watch face, ported from wasp-os's fibonacci_clock.
// Five squares of sides 1, 1, 2, 3 and 5: red ones add up to the hour, green ones to the minutes in fives,
// blue ones count for both, and white ones for neither.

import { computed } from "@preact/signals";
import {
  battery_percent,
  define_app,
  format_time,
  hour_of,
  is_charging,
  is_phone_connected,
  minute_of,
  now,
  unread_notifications,
  type Color,
} from "neotime";
import { icons } from "neotime/icons";
import { Column, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** White for neither, red for hours, green for minutes and blue for both. */
const SQUARE_COLORS: Color[] = ["#ffffff", "#ff0000", "#00ff00", "#0000ff"];

// ============================================================================
// Derived values
// ============================================================================

/**
 * Splits a number from 0 to 12 into the squares 5, 3, 2, 1 and 1, largest first,
 * as a mask with 1 for the 5, 2 for the 3, 4 for the 2, 8 for the first 1 and 16 for the second.
 */
function fibonacci_mask(value: number): number {
  const after_five = value >= 5 ? value - 5 : value;
  const after_three = after_five >= 3 ? after_five - 3 : after_five;
  const after_two = after_three >= 2 ? after_three - 2 : after_three;
  const after_first_one = after_two >= 1 ? after_two - 1 : after_two;
  return (
    (value >= 5 ? 1 : 0) +
    (after_five >= 3 ? 2 : 0) +
    (after_three >= 2 ? 4 : 0) +
    (after_two >= 1 ? 8 : 0) +
    (after_first_one >= 1 ? 16 : 0)
  );
}

const hour_mask = computed(() => fibonacci_mask(hour_of(now.value) % 12));

/** The clock shows minutes in steps of five. */
const minute_mask = computed(() => fibonacci_mask(Math.floor(minute_of(now.value) / 5)));

/** Green when well charged, then yellow, then red, as wasp-os's battery meter. */
const battery_color = computed<Color>(() =>
  battery_percent.value > 50 ? "#00c000" : battery_percent.value > 20 ? "#e0c000" : "#e00000",
);

// ============================================================================
// Screen
// ============================================================================

function StatusBar() {
  return (
    <Row x={0} y={0} width={240} height={32} justify="end" gap={6} padding={4}>
      {unread_notifications.value > 0 && <Icon icon={icons.notification} size={22} color="accent" />}
      {is_phone_connected.value && <Icon icon={icons.bluetooth} size={22} color="accent" />}
      {is_charging.value ? (
        <Icon icon={icons.charging} size={22} color="warning" />
      ) : (
        <Row width={26} height={14} border_width={2} border_color={battery_color.value} radius={2} padding={1}>
          <Column
            width={Math.max(1, Math.floor((battery_percent.value * 20) / 100))}
            height={8}
            background={battery_color.value}
          />
        </Row>
      )}
    </Row>
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
