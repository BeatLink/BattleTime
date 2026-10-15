// OmegaTime watch face, ported from jlukanc1's OmegaTime in ZephyrLabs' Watchfaces collection (ZephyrLabs/Watchfaces).
// A glowing cyan dial on black: round dots for the hours, bars at 3, 6 and 9 and a double bar at 12.
// Outlined arrow hands, a grey second hand carrying a cyan dot, and the day of the month and weekday in grey.

import {
  computed,
  day_of,
  define_app,
  format_number,
  format_time,
  hour_of,
  minute_of,
  now,
  second_of,
  to_upper,
  unread_notifications,
  type Color,
} from "battletime";
import { BatteryIcon } from "battletime/components";
import { icons } from "battletime/icons";
import { Column, Icon, Label, Line, Row, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

const BLACK: Color = "#000000";
const CYAN: Color = "#00ffff";
const GREY: Color = "#808080";

/** OmegaTime's hand lengths in pixels from the centre; each arrow's point reaches a fifth further. */
const HOUR_LENGTH = 60;
const MINUTE_LENGTH = 70;
const SECOND_LENGTH = 90;
const HOUR_TIP_LENGTH = 72;
const MINUTE_TIP_LENGTH = 84;

/** How far behind the centre the second hand starts, and how far out its dot sits. */
const SECOND_TAIL_LENGTH = 20;
const SECOND_DOT_DISTANCE = 70;

/** How far from the centre each hand's outline starts. */
const HAND_BASE_LENGTH = 5;

const HOUR_DOT_SIZE = 16;
const HOUR_DOT_RADIUS = 8;
const SECOND_DOT_SIZE = 8;
const SECOND_DOT_HALF = 4;

/** The sine of each six-degree step round the dial, in thousandths, from twelve o'clock clockwise. */
const SINE = [
  0, 105, 208, 309, 407, 500, 588, 669, 743, 809, 866, 914, 951, 978, 995, 1000, 995, 978, 951, 914, 866, 809, 743,
  669, 588, 500, 407, 309, 208, 105, 0, -105, -208, -309, -407, -500, -588, -669, -743, -809, -866, -914, -951, -978,
  -995, -1000, -995, -978, -951, -914, -866, -809, -743, -669, -588, -500, -407, -309, -208, -105,
];

/** The centres of the dots at every hour without a bar, 110 pixels out. */
const HOUR_DOTS = [
  { x: 175, y: 25 },
  { x: 215, y: 65 },
  { x: 215, y: 175 },
  { x: 175, y: 215 },
  { x: 65, y: 215 },
  { x: 25, y: 175 },
  { x: 25, y: 65 },
  { x: 65, y: 25 },
];

// ============================================================================
// Derived values
// ============================================================================

/** The minute hand's place in sixty steps round the dial. */
const minute_step = computed(() => minute_of(now.value));

/** The hour hand's place in sixty steps, moving on a step every twelve minutes. */
const hour_step = computed(() => (hour_of(now.value) % 12) * 5 + Math.floor(minute_of(now.value) / 12));

const second_step = computed(() => second_of(now.value));

const day_text = computed(() => format_number(day_of(now.value), 2));

const weekday_text = computed(() => to_upper(format_time(now.value, "ddd")));

// ============================================================================
// Geometry
// ============================================================================

/** The x of the point `radius` pixels from the centre at `step` of sixty round the dial. */
function x_at(radius: number, step: number): number {
  return 120 + Math.trunc((radius * SINE[step % 60]) / 1000);
}

/** The y of the point `radius` pixels from the centre at `step` of sixty round the dial. */
function y_at(radius: number, step: number): number {
  return 120 - Math.trunc((radius * SINE[(step + 15) % 60]) / 1000);
}

// ============================================================================
// Screen
// ============================================================================

/** The minute hand: two sides from the centre, a crossbar and a point beyond it, outlined in cyan. */
function MinuteHand() {
  return (
    <Line
      points={[
        { x: x_at(HAND_BASE_LENGTH, minute_step.value + 59), y: y_at(HAND_BASE_LENGTH, minute_step.value + 59) },
        { x: x_at(MINUTE_LENGTH, minute_step.value + 59), y: y_at(MINUTE_LENGTH, minute_step.value + 59) },
        { x: x_at(MINUTE_TIP_LENGTH, minute_step.value), y: y_at(MINUTE_TIP_LENGTH, minute_step.value) },
        { x: x_at(MINUTE_LENGTH, minute_step.value + 1), y: y_at(MINUTE_LENGTH, minute_step.value + 1) },
        { x: x_at(MINUTE_LENGTH, minute_step.value + 59), y: y_at(MINUTE_LENGTH, minute_step.value + 59) },
        { x: x_at(MINUTE_LENGTH, minute_step.value + 1), y: y_at(MINUTE_LENGTH, minute_step.value + 1) },
        { x: x_at(HAND_BASE_LENGTH, minute_step.value + 1), y: y_at(HAND_BASE_LENGTH, minute_step.value + 1) },
      ]}
      thickness={2}
      color={CYAN}
    />
  );
}

/** The hour hand, shaped like the minute hand; two uses of one component would share one point list. */
function HourHand() {
  return (
    <Line
      points={[
        { x: x_at(HAND_BASE_LENGTH, hour_step.value + 59), y: y_at(HAND_BASE_LENGTH, hour_step.value + 59) },
        { x: x_at(HOUR_LENGTH, hour_step.value + 59), y: y_at(HOUR_LENGTH, hour_step.value + 59) },
        { x: x_at(HOUR_TIP_LENGTH, hour_step.value), y: y_at(HOUR_TIP_LENGTH, hour_step.value) },
        { x: x_at(HOUR_LENGTH, hour_step.value + 1), y: y_at(HOUR_LENGTH, hour_step.value + 1) },
        { x: x_at(HOUR_LENGTH, hour_step.value + 59), y: y_at(HOUR_LENGTH, hour_step.value + 59) },
        { x: x_at(HOUR_LENGTH, hour_step.value + 1), y: y_at(HOUR_LENGTH, hour_step.value + 1) },
        { x: x_at(HAND_BASE_LENGTH, hour_step.value + 1), y: y_at(HAND_BASE_LENGTH, hour_step.value + 1) },
      ]}
      thickness={2}
      color={CYAN}
    />
  );
}

function Dial() {
  return (
    <>
      {HOUR_DOTS.map((dot) => (
        <Column
          x={dot.x - HOUR_DOT_RADIUS}
          y={dot.y - HOUR_DOT_RADIUS}
          width={HOUR_DOT_SIZE}
          height={HOUR_DOT_SIZE}
          radius={HOUR_DOT_RADIUS}
          background={CYAN}
        />
      ))}
      <Line points={[{ x: 112, y: 8 }, { x: 112, y: 32 }]} thickness={8} color={CYAN} />
      <Line points={[{ x: 128, y: 8 }, { x: 128, y: 32 }]} thickness={8} color={CYAN} />
      <Line points={[{ x: 208, y: 120 }, { x: 232, y: 120 }]} thickness={8} color={CYAN} />
      <Line points={[{ x: 120, y: 208 }, { x: 120, y: 232 }]} thickness={8} color={CYAN} />
      <Line points={[{ x: 8, y: 120 }, { x: 32, y: 120 }]} thickness={8} color={CYAN} />
    </>
  );
}

function OmegaTimeFace() {
  const second = second_step.value;
  return (
    <Screen background={BLACK}>
      <Dial />

      <Label x={30} y={111} width={60} text_align="center" color={GREY}>
        {day_text.value}
      </Label>
      <Label x={130} y={111} width={60} text_align="center" color={GREY}>
        {weekday_text.value}
      </Label>

      {unread_notifications.value > 0 && <Icon x={8} y={214} icon={icons.notification} size={20} color={CYAN} />}
      <Row x={176} y={216} width={56} height={20} justify="end">
        <BatteryIcon single_color />
      </Row>

      <Line
        points={[
          { x: x_at(SECOND_TAIL_LENGTH, second + 30), y: y_at(SECOND_TAIL_LENGTH, second + 30) },
          { x: x_at(SECOND_LENGTH, second), y: y_at(SECOND_LENGTH, second) },
        ]}
        thickness={3}
        color={GREY}
      />
      <Column
        x={x_at(SECOND_DOT_DISTANCE, second) - SECOND_DOT_HALF}
        y={y_at(SECOND_DOT_DISTANCE, second) - SECOND_DOT_HALF}
        width={SECOND_DOT_SIZE}
        height={SECOND_DOT_SIZE}
        radius={SECOND_DOT_HALF}
        background={CYAN}
      />
      <MinuteHand />
      <HourHand />
    </Screen>
  );
}

export default define_app({
  id: "omega-time",
  name: "OmegaTime",
  icon: icons.clock,
  kind: "face",
  screens: { main: OmegaTimeFace },
});
