// Phrase clock watch face, ported from ZephyrLabs' Wordclock (ZephyrLabs/Watchfaces).
// The time is a lower-case phrase such as "nineteen minutes / to nine" over a dark shadow, with the date below it.
// The phrasing follows the original exactly: hours past the hour count to 23, hours to the hour wrap at twelve.

import {
  computed,
  day_of,
  define_app,
  heart_rate,
  hour_of,
  is_heart_rate_running,
  minute_of,
  month_of,
  now,
  unread_notifications,
  weekday_of,
  year_of,
  type Color,
} from "battletime";
import { StatusIcons, StepsBadge } from "battletime/components";
import { icons } from "battletime/icons";
import { Badge, Column, Icon, Label, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

const SHADOW_COLOR: Color = "#404040";

/** The original's number words, enough for any hour of the day and any minute up to the half hour. */
const NUMBER_WORDS = [
  "zero",
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
  "thirteen",
  "fourteen",
  "fifteen",
  "sixteen",
  "seventeen",
  "eighteen",
  "nineteen",
  "twenty",
  "twenty one",
  "twenty two",
  "twenty three",
  "twenty four",
  "twenty five",
  "twenty six",
  "twenty seven",
  "twenty eight",
  "twenty nine",
];

/** Weekdays from Monday, as the original abbreviates them. */
const DAY_NAMES = ["mon.", "tue.", "wed.", "thu.", "fri.", "sat.", "sun."];

const MONTH_NAMES = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

// ============================================================================
// Derived values
// ============================================================================

const hour = computed(() => hour_of(now.value));

const minute = computed(() => minute_of(now.value));

/** The hour named by "to", which the original counts on a twelve hour dial. */
const coming_hour_word = computed(() => NUMBER_WORDS[(hour.value % 12) + 1]);

const hour_word = computed(() => NUMBER_WORDS[hour.value]);

const first_line = computed(() => {
  const m = minute.value;
  if (m === 0) return hour_word.value;
  if (m === 1 || m === 59) return "one minute";
  if (m === 15) return "quarter past";
  if (m === 30) return "half past";
  if (m === 45) return "quarter to";
  if (m < 30) return `${NUMBER_WORDS[m]} minutes`;
  return `${NUMBER_WORDS[60 - m]} minutes`;
});

const second_line = computed(() => {
  const m = minute.value;
  if (m === 0) return "o' clock";
  if (m === 15 || m === 30) return hour_word.value;
  if (m === 45) return coming_hour_word.value;
  if (m < 30) return `past ${hour_word.value}`;
  return `to ${coming_hour_word.value}`;
});

const date_text = computed(
  () =>
    `${DAY_NAMES[weekday_of(now.value)]} ${day_of(now.value)} ${MONTH_NAMES[month_of(now.value) - 1]} ${year_of(now.value)}`,
);

const heart_rate_text = computed(() => (is_heart_rate_running.value ? `${heart_rate.value} BPM` : "--- BPM"));

// ============================================================================
// Screen
// ============================================================================

function Phrase(props: { x: number; y: number; color: Color }) {
  return (
    <Column x={props.x} y={props.y} gap={2}>
      <Label color={props.color}>{first_line.value}</Label>
      <Label color={props.color}>{second_line.value}</Label>
    </Column>
  );
}

function PhraseClockFace() {
  return (
    <Screen>
      {unread_notifications.value > 0 && <Icon x={10} y={0} icon={icons.notification} size={20} />}
      <StatusIcons x={0} y={4} width={235} height={20} align="end" gap={5} icon_size={20} show_notifications={false} />

      <Phrase x={7} y={87} color={SHADOW_COLOR} />
      <Phrase x={5} y={85} color="#ffffff" />

      <Label x={10} y={170}>{date_text.value}</Label>

      <Badge x={5} y={214} height={24} icon={icons.heart} icon_size={20} gap={5}>
        {heart_rate_text.value}
      </Badge>
      <StepsBadge x={120} y={214} width={115} height={24} align="end" icon_size={20} />
    </Screen>
  );
}

export default define_app({
  id: "phrase-clock",
  name: "Phrase clock",
  icon: icons.clock,
  kind: "face",
  screens: { main: PhraseClockFace },
});
