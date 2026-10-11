// Word clock watch face, ported from wasp-os's word_clock.
// The time is written out in words, such as "Just gone / quarter past / three / in the afternoon", over the date.

import { computed } from "@preact/signals";
import {
  battery_percent,
  define_app,
  format_time,
  hour_of,
  is_phone_connected,
  minute_of,
  now,
  unread_notifications,
  type Color,
} from "neotime";
import { BatteryIcon } from "neotime/components";
import { icons } from "neotime/icons";
import { Column, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** How far past the last five minutes, by the minute's remainder after dividing by five. */
const NEARNESS_WORDS = ["", "Just gone", "After", "Coming up to", "Almost"];

/** The five minute step, by the minute plus two divided by five. */
const STEP_WORDS = [
  "",
  "five past",
  "ten past",
  "quarter past",
  "twenty past",
  "twenty-five past",
  "half past",
  "twenty-five to",
  "twenty to",
  "quarter to",
  "ten to",
  "five to",
  "",
];

const CAPITALISED_STEP_WORDS = [
  "",
  "Five past",
  "Ten past",
  "Quarter past",
  "Twenty past",
  "Twenty-five past",
  "Half past",
  "Twenty-five to",
  "Twenty to",
  "Quarter to",
  "Ten to",
  "Five to",
  "",
];

const HOUR_WORDS = [
  "midnight",
  "one",
  "two",
  "three",
  "four",
  "five",
  "six",
  "seven",
  "eight",
  "nine",
  "ten",
  "eleven",
  "twelve",
  "one",
  "two",
  "three",
  "four",
  "five",
  "six",
  "seven",
  "eight",
  "nine",
  "ten",
  "eleven",
];

const CAPITALISED_HOUR_WORDS = [
  "Midnight",
  "One",
  "Two",
  "Three",
  "Four",
  "Five",
  "Six",
  "Seven",
  "Eight",
  "Nine",
  "Ten",
  "Eleven",
  "Twelve",
  "One",
  "Two",
  "Three",
  "Four",
  "Five",
  "Six",
  "Seven",
  "Eight",
  "Nine",
  "Ten",
  "Eleven",
];

// ============================================================================
// Derived values
// ============================================================================

const hour = computed(() => hour_of(now.value));

const minute = computed(() => minute_of(now.value));

const nearness_text = computed(() => NEARNESS_WORDS[minute.value % 5]);

const step = computed(() => Math.floor((minute.value + 2) / 5));

/** Past the half hour the words name the coming hour. */
const named_hour = computed(() => (minute.value > 32 ? (hour.value + 1) % 24 : hour.value));

const step_text = computed(() =>
  nearness_text.value === "" ? CAPITALISED_STEP_WORDS[step.value] : STEP_WORDS[step.value],
);

const hour_text = computed(() =>
  nearness_text.value === "" && STEP_WORDS[step.value] === ""
    ? CAPITALISED_HOUR_WORDS[named_hour.value]
    : HOUR_WORDS[named_hour.value],
);

/** Midnight and noon need no part of the day. */
const part_of_day_text = computed(() =>
  named_hour.value === 0 || named_hour.value === 12
    ? ""
    : hour.value >= 22
      ? "at night"
      : hour.value >= 18
        ? "in the evening"
        : hour.value >= 12
          ? "in the afternoon"
          : hour.value >= 6
            ? "in the morning"
            : hour.value >= 3
              ? "in the early hours"
              : "at night",
);

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
      <BatteryIcon color={battery_color.value} />
    </Row>
  );
}

function WordClockFace() {
  return (
    <Screen>
      <StatusBar />
      <Column x={0} y={40} width={240} height={168} justify="center" align_items="center" gap={4}>
        {nearness_text.value !== "" && <Label font="normal">{nearness_text.value}</Label>}
        {step_text.value !== "" && <Label font="normal">{step_text.value}</Label>}
        <Label font="normal">{hour_text.value}</Label>
        {part_of_day_text.value !== "" && <Label font="normal">{part_of_day_text.value}</Label>}
      </Column>
      <Label x={0} y={214} width={240} text_align="center">
        {format_time(now.value, "D MMM YYYY")}
      </Label>
    </Screen>
  );
}

export default define_app({
  id: "word-clock",
  name: "Word clock",
  icon: icons.clock,
  kind: "face",
  screens: { main: WordClockFace },
});
