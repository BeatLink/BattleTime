// Casio G-7710 style watch face, ported from InfiniTime's WatchFaceCasioStyleG7710.
// Green-grey text on black: weekday and week number in a box, day of the year and days left, the date and a big time.
// On a 24 hour clock the date is day-month and weeks start on Monday; on a 12 hour clock it is month-day and weeks start on Sunday.

import { computed } from "@preact/signals";
import {
  battery_percent,
  day_of,
  define_app,
  format_number,
  heart_rate,
  hour_of,
  is_charging,
  is_phone_connected,
  minute_of,
  month_of,
  now,
  settings,
  steps_today,
  unread_notifications,
  weekday_of,
  year_of,
  type Color,
} from "neotime";
import { icons } from "neotime/icons";
import { Column, Icon, Label, Line, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const TEXT_COLOR: Color = "#98b69a";
const HEART_OFF_COLOR: Color = "#1b1b1b";

const WEEKDAY_NAMES = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];

/** Days in the months before each month of a common year. */
const DAYS_BEFORE_MONTH = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

// ============================================================================
// Derived values
// ============================================================================

function is_leap_year(year: number): boolean {
  return (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
}

/** The weekday of 31 December of a year, 0 for Sunday, used to count ISO weeks. */
function year_end_weekday(year: number): number {
  return (year + Math.floor(year / 4) - Math.floor(year / 100) + Math.floor(year / 400)) % 7;
}

function iso_weeks_in_year(year: number): number {
  return year_end_weekday(year) === 4 || year_end_weekday(year - 1) === 3 ? 53 : 52;
}

/** A whole number padded with spaces to a width of up to three, as printf's `%3d` does. */
function pad_with_spaces(value: number, width: number): string {
  const padding = width >= 3 && value < 100 ? (value < 10 ? "  " : " ") : width === 2 && value < 10 ? " " : "";
  return `${padding}${value}`;
}

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

const year = computed(() => year_of(now.value));

const day_of_year = computed(
  () =>
    DAYS_BEFORE_MONTH[month_of(now.value) - 1] +
    day_of(now.value) +
    (month_of(now.value) > 2 && is_leap_year(year.value) ? 1 : 0),
);

const days_left_in_year = computed(() => (is_leap_year(year.value) ? 366 : 365) - day_of_year.value);

/** The ISO 8601 week, with Monday as the first day, as strftime's `%V` gives. */
const iso_week = computed(() => {
  const week = Math.floor((day_of_year.value - (weekday_of(now.value) + 1) + 10) / 7);
  return week < 1 ? iso_weeks_in_year(year.value - 1) : week > iso_weeks_in_year(year.value) ? 1 : week;
});

/** The week counted from the first Sunday of the year, as strftime's `%U` gives. */
const sunday_week = computed(() => Math.floor((day_of_year.value + 6 - ((weekday_of(now.value) + 1) % 7)) / 7));

const week_number = computed(() => (uses_24_hour_clock.value ? iso_week.value : sunday_week.value));

const date_text = computed(() =>
  uses_24_hour_clock.value
    ? `${pad_with_spaces(day_of(now.value), 3)}-${pad_with_spaces(month_of(now.value), 2)}`
    : `${pad_with_spaces(month_of(now.value), 3)}-${pad_with_spaces(day_of(now.value), 2)}`,
);

const display_hour = computed(() => {
  const hour = hour_of(now.value);
  return uses_24_hour_clock.value ? hour : hour % 12 === 0 ? 12 : hour % 12;
});

const time_text = computed(
  () =>
    `${uses_24_hour_clock.value ? format_number(display_hour.value, 2) : pad_with_spaces(display_hour.value, 2)}:${format_number(minute_of(now.value), 2)}`,
);

const is_heart_rate_shown = computed(() => heart_rate.value > 0);

// ============================================================================
// Screen
// ============================================================================

function BatteryLevel() {
  return (
    <Row width={26} height={14} border_width={2} border_color={TEXT_COLOR} radius={2} padding={1}>
      <Column
        width={Math.max(1, Math.floor((battery_percent.value * 20) / 100))}
        height={8}
        background={TEXT_COLOR}
      />
    </Row>
  );
}

function CasioFace() {
  return (
    <Screen>
      <Row x={60} y={0} width={180} height={20} justify="end" gap={5}>
        {unread_notifications.value > 0 && <Icon icon={icons.notification} size={18} color={TEXT_COLOR} />}
        {is_phone_connected.value && <Icon icon={icons.bluetooth} size={18} color={TEXT_COLOR} />}
        {is_charging.value && <Icon icon={icons.charging} size={18} color={TEXT_COLOR} />}
        <BatteryLevel />
        <Label font="small" color={TEXT_COLOR}>{`${battery_percent.value}%`}</Label>
      </Row>
      <Line points={[{ x: 107, y: 23 }, { x: 224, y: 23 }, { x: 229, y: 18 }]} color={TEXT_COLOR} />

      <Line
        points={[{ x: 0, y: 8 }, { x: 100, y: 8 }, { x: 95, y: 103 }, { x: 0, y: 103 }]}
        thickness={6}
        color={TEXT_COLOR}
      />
      <Label x={5} y={22} font="large" color={TEXT_COLOR}>
        {`WK${format_number(week_number.value, 2)}`}
      </Label>
      <Label x={10} y={60} font="large" color={TEXT_COLOR}>
        {WEEKDAY_NAMES[weekday_of(now.value)]}
      </Label>

      <Label x={110} y={40} color={TEXT_COLOR}>
        {`${pad_with_spaces(day_of_year.value, 3)}-${pad_with_spaces(days_left_in_year.value, 3)}`}
      </Label>
      <Line points={[{ x: 104, y: 65 }, { x: 234, y: 65 }, { x: 239, y: 60 }]} color={TEXT_COLOR} />

      <Label x={88} y={62} font="large" color={TEXT_COLOR}>
        {date_text.value}
      </Label>
      <Line points={[{ x: 99, y: 105 }, { x: 234, y: 105 }, { x: 239, y: 100 }]} color={TEXT_COLOR} />

      {!uses_24_hour_clock.value && (
        <Label x={5} y={108} color={TEXT_COLOR}>
          {hour_of(now.value) >= 12 ? "P" : "A"}
        </Label>
      )}
      <Label x={0} y={118} width={240} text_align="center" font="huge" color={TEXT_COLOR}>
        {time_text.value}
      </Label>
      <Line points={[{ x: 4, y: 209 }, { x: 234, y: 209 }, { x: 239, y: 214 }]} color={TEXT_COLOR} />

      <Row x={5} y={214} height={24} gap={5}>
        <Icon icon={icons.heart} size={20} color={is_heart_rate_shown.value ? TEXT_COLOR : HEART_OFF_COLOR} />
        {is_heart_rate_shown.value && <Label color={TEXT_COLOR}>{`${heart_rate.value}`}</Label>}
      </Row>
      <Row x={120} y={214} width={115} height={24} justify="end" gap={5}>
        <Icon icon={icons.steps} size={20} color={TEXT_COLOR} />
        <Label color={TEXT_COLOR}>{`${steps_today.value}`}</Label>
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "casio",
  name: "Casio G7710",
  icon: icons.clock,
  kind: "face",
  screens: { main: CasioFace },
});
