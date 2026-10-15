// Fancy watch face, ported from jlukanc1's Fancy in ZephyrLabs' Watchfaces collection (ZephyrLabs/Watchfaces).
// A grey and black lattice behind either an analog dial with outlined arrow hands or big digits over the date.
// The original was two faces; here a long press switches between the two looks and the choice is kept.

import {
  computed,
  day_of,
  define_app,
  format_number,
  format_time,
  hour_of,
  is_heart_rate_running,
  minute_of,
  now,
  persisted,
  second_of,
  settings,
  to_upper,
  unread_notifications,
  type Color,
} from "battletime";
import { display_hour } from "battletime/lib";
import { HeartRateBadge, StatusIcons, StepsBadge } from "battletime/components";
import { icons } from "battletime/icons";
import { Column, Icon, Label, Line, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

type Look = "analog" | "digital";

const GREY: Color = "#737373";
const BLACK: Color = "#000000";
const WHITE: Color = "#ffffff";
const SKY_BLUE: Color = "#87cefa";
const HEART_OFF_COLOR: Color = "#303030";

/** Fancy's hand lengths in pixels from the centre; each arrow's point reaches a fifth further. */
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

const SECOND_DOT_SIZE = 8;
const SECOND_DOT_HALF = 4;

/** The sine of each six-degree step round the dial, in thousandths, from twelve o'clock clockwise. */
const SINE = [
  0, 105, 208, 309, 407, 500, 588, 669, 743, 809, 866, 914, 951, 978, 995, 1000, 995, 978, 951, 914, 866, 809, 743,
  669, 588, 500, 407, 309, 208, 105, 0, -105, -208, -309, -407, -500, -588, -669, -743, -809, -866, -914, -951, -978,
  -995, -1000, -995, -978, -951, -914, -866, -809, -743, -669, -588, -500, -407, -309, -208, -105,
];

/** The black lattice over the grey: lines sloping both ways 16 pixels apart, joined off the screen's sides. */
const LATTICE = [
  { x: -20, y: -126 }, { x: 260, y: 14 }, { x: 260, y: -126 }, { x: -20, y: 14 },
  { x: -20, y: -110 }, { x: 260, y: 30 }, { x: 260, y: -110 }, { x: -20, y: 30 },
  { x: -20, y: -94 }, { x: 260, y: 46 }, { x: 260, y: -94 }, { x: -20, y: 46 },
  { x: -20, y: -78 }, { x: 260, y: 62 }, { x: 260, y: -78 }, { x: -20, y: 62 },
  { x: -20, y: -62 }, { x: 260, y: 78 }, { x: 260, y: -62 }, { x: -20, y: 78 },
  { x: -20, y: -46 }, { x: 260, y: 94 }, { x: 260, y: -46 }, { x: -20, y: 94 },
  { x: -20, y: -30 }, { x: 260, y: 110 }, { x: 260, y: -30 }, { x: -20, y: 110 },
  { x: -20, y: -14 }, { x: 260, y: 126 }, { x: 260, y: -14 }, { x: -20, y: 126 },
  { x: -20, y: 2 }, { x: 260, y: 142 }, { x: 260, y: 2 }, { x: -20, y: 142 },
  { x: -20, y: 18 }, { x: 260, y: 158 }, { x: 260, y: 18 }, { x: -20, y: 158 },
  { x: -20, y: 34 }, { x: 260, y: 174 }, { x: 260, y: 34 }, { x: -20, y: 174 },
  { x: -20, y: 50 }, { x: 260, y: 190 }, { x: 260, y: 50 }, { x: -20, y: 190 },
  { x: -20, y: 66 }, { x: 260, y: 206 }, { x: 260, y: 66 }, { x: -20, y: 206 },
  { x: -20, y: 82 }, { x: 260, y: 222 }, { x: 260, y: 82 }, { x: -20, y: 222 },
  { x: -20, y: 98 }, { x: 260, y: 238 }, { x: 260, y: 98 }, { x: -20, y: 238 },
  { x: -20, y: 114 }, { x: 260, y: 254 }, { x: 260, y: 114 }, { x: -20, y: 254 },
  { x: -20, y: 130 }, { x: 260, y: 270 }, { x: 260, y: 130 }, { x: -20, y: 270 },
  { x: -20, y: 146 }, { x: 260, y: 286 }, { x: 260, y: 146 }, { x: -20, y: 286 },
  { x: -20, y: 162 }, { x: 260, y: 302 }, { x: 260, y: 162 }, { x: -20, y: 302 },
  { x: -20, y: 178 }, { x: 260, y: 318 }, { x: 260, y: 178 }, { x: -20, y: 318 },
  { x: -20, y: 194 }, { x: 260, y: 334 }, { x: 260, y: 194 }, { x: -20, y: 334 },
  { x: -20, y: 210 }, { x: 260, y: 350 }, { x: 260, y: 210 }, { x: -20, y: 350 },
  { x: -20, y: 226 }, { x: 260, y: 366 }, { x: 260, y: 226 }, { x: -20, y: 366 },
];

/** Which of Fancy's two looks is shown. */
const look = persisted<Look>("look", "analog");

// ============================================================================
// Derived values
// ============================================================================

/** The minute hand's place in sixty steps round the dial. */
const minute_step = computed(() => minute_of(now.value));

/** The hour hand's place in sixty steps, moving on a step every twelve minutes. */
const hour_step = computed(() => (hour_of(now.value) % 12) * 5 + Math.floor(minute_of(now.value) / 12));

const second_step = computed(() => second_of(now.value));

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

/** The time with two hour digits on a 24 hour clock, and a space in place of a leading zero on a 12 hour one. */
const time_text = computed(() => {
  const hour = display_hour(now.value);
  const hour_text = uses_24_hour_clock.value || hour >= 10 ? format_number(hour, 2) : ` ${hour}`;
  return `${hour_text}:${format_number(minute_of(now.value), 2)}`;
});

/** The weekday, day, month and year, the day before the month on a 24 hour clock, as Fancy writes it. */
const date_text = computed(() =>
  to_upper(format_time(now.value, uses_24_hour_clock.value ? "ddd D MMM YYYY" : "ddd MMM D YYYY")),
);

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
// Handlers
// ============================================================================

function switch_look() {
  look.value = look.value === "analog" ? "digital" : "analog";
}

// ============================================================================
// Screen
// ============================================================================

/** A hand: two sides from the centre, a crossbar `length` out and a point at `tip`, outlined in white. */
function ArrowHand(props: { step: number; length: number; tip: number }) {
  return (
    <Line
      points={[
        { x: x_at(HAND_BASE_LENGTH, props.step + 59), y: y_at(HAND_BASE_LENGTH, props.step + 59) },
        { x: x_at(props.length, props.step + 59), y: y_at(props.length, props.step + 59) },
        { x: x_at(props.tip, props.step), y: y_at(props.tip, props.step) },
        { x: x_at(props.length, props.step + 1), y: y_at(props.length, props.step + 1) },
        { x: x_at(props.length, props.step + 59), y: y_at(props.length, props.step + 59) },
        { x: x_at(props.length, props.step + 1), y: y_at(props.length, props.step + 1) },
        { x: x_at(HAND_BASE_LENGTH, props.step + 1), y: y_at(HAND_BASE_LENGTH, props.step + 1) },
      ]}
      thickness={2}
      color={WHITE}
    />
  );
}

/** White bars at 3, 6 and 9 and a double bar at 12, the day and weekday, and the hands with the second hand's dot. */
function AnalogLook() {
  const second = second_step.value;
  return (
    <>
      <Line points={[{ x: 112, y: 8 }, { x: 112, y: 32 }]} thickness={8} color={WHITE} />
      <Line points={[{ x: 128, y: 8 }, { x: 128, y: 32 }]} thickness={8} color={WHITE} />
      <Line points={[{ x: 212, y: 120 }, { x: 236, y: 120 }]} thickness={8} color={WHITE} />
      <Line points={[{ x: 120, y: 212 }, { x: 120, y: 236 }]} thickness={8} color={WHITE} />
      <Line points={[{ x: 8, y: 120 }, { x: 32, y: 120 }]} thickness={8} color={WHITE} />

      <Label x={30} y={111} width={60} text_align="center" color={SKY_BLUE}>
        {format_number(day_of(now.value), 2)}
      </Label>
      <Label x={130} y={111} width={60} text_align="center" color={SKY_BLUE}>
        {to_upper(format_time(now.value, "ddd"))}
      </Label>

      {unread_notifications.value > 0 && <Icon x={6} y={2} icon={icons.notification} size={20} color={WHITE} />}
      <StatusIcons
        x={160}
        y={0}
        width={76}
        height={24}
        align="end"
        icon_size={18}
        color={WHITE}
        show_notifications={false}
        show_phone={false}
        single_color
      />

      <Line
        points={[
          { x: x_at(SECOND_TAIL_LENGTH, second + 30), y: y_at(SECOND_TAIL_LENGTH, second + 30) },
          { x: x_at(SECOND_LENGTH, second), y: y_at(SECOND_LENGTH, second) },
        ]}
        thickness={3}
        color={SKY_BLUE}
      />
      <Column
        x={x_at(SECOND_DOT_DISTANCE, second) - SECOND_DOT_HALF}
        y={y_at(SECOND_DOT_DISTANCE, second) - SECOND_DOT_HALF}
        width={SECOND_DOT_SIZE}
        height={SECOND_DOT_SIZE}
        radius={SECOND_DOT_HALF}
        background={WHITE}
      />
      <ArrowHand step={minute_step.value} length={MINUTE_LENGTH} tip={MINUTE_TIP_LENGTH} />
      <ArrowHand step={hour_step.value} length={HOUR_LENGTH} tip={HOUR_TIP_LENGTH} />
    </>
  );
}

/** Big sky blue digits, AM or PM on a 12 hour clock and the date, with the status, heart rate and steps in white. */
function DigitalLook() {
  return (
    <>
      <StatusIcons x={0} y={0} width={236} height={24} align="end" gap={6} icon_size={18} color={WHITE} single_color />
      {!uses_24_hour_clock.value && (
        <Label x={180} y={50} width={50} text_align="right" color={SKY_BLUE}>
          {hour_of(now.value) >= 12 ? "PM" : "AM"}
        </Label>
      )}
      <Label x={0} y={84} width={240} text_align="center" font="huge" color={SKY_BLUE}>
        {time_text.value}
      </Label>
      <Label x={0} y={170} width={240} text_align="center" color={SKY_BLUE}>
        {date_text.value}
      </Label>
      <HeartRateBadge
        x={5}
        y={214}
        height={24}
        color={WHITE}
        icon_color={is_heart_rate_running.value ? WHITE : HEART_OFF_COLOR}
      />
      <StepsBadge x={120} y={214} width={115} height={24} align="end" color={WHITE} />
    </>
  );
}

function FancyFace() {
  return (
    <Screen background={GREY} onLongPress={switch_look}>
      <Line points={LATTICE} thickness={5} color={BLACK} rounded={false} />
      {look.value === "analog" ? <AnalogLook /> : <DigitalLook />}
    </Screen>
  );
}

export default define_app({
  id: "fancy",
  name: "Fancy",
  icon: icons.clock,
  kind: "face",
  screens: { main: FancyFace },
});
