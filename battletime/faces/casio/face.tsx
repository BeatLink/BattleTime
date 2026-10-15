// Casio G-7710 style watch face, ported from InfiniTime's WatchFaceCasioStyleG7710.
// Green-grey text on black: weekday and week number in a box, day of the year and days left, the date and a big 7-segment time.
// On a 24 hour clock the date is day-month and weeks start on Monday; on a 12 hour clock it is month-day and weeks start on Sunday.

import {
  battery_percent,
  computed,
  day_of,
  day_of_year,
  define_app,
  format_time,
  hour_of,
  is_heart_rate_running,
  is_phone_connected,
  iso_week_of,
  month_of,
  now,
  pad_start,
  settings,
  to_upper,
  unread_notifications,
  year_of,
  type Color,
} from "battletime";
import { days_in_year, sunday_week_of } from "battletime/lib";
import { BatteryIcon, HeartRateBadge, StepsBadge } from "battletime/components";
import { icons } from "battletime/icons";
import { Icon, Label, Line, Row, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

const TEXT_COLOR: Color = "#98b69a";
const HEART_OFF_COLOR: Color = "#1b1b1b";

// ============================================================================
// Derived values
// ============================================================================

/** The ISO week on a 24 hour clock and the Sunday week on a 12 hour clock, as InfiniTime picks. */
export function week_number_of(instant: number, uses_24_hour_clock: boolean): number {
  return uses_24_hour_clock ? iso_week_of(instant) : sunday_week_of(instant);
}

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

const day_number = computed(() => day_of_year(now.value));

const days_left_in_year = computed(() => days_in_year(year_of(now.value)) - day_number.value);

/** The day and month as printf's `%3d-%2d`, day first on a 24 hour clock and month first on a 12 hour one. */
const date_text = computed(() => {
  const first = uses_24_hour_clock.value ? day_of(now.value) : month_of(now.value);
  const second = uses_24_hour_clock.value ? month_of(now.value) : day_of(now.value);
  return `${pad_start(`${first}`, 3)}-${pad_start(`${second}`, 2)}`;
});

/** InfiniTime pads a 12 hour clock's hour with a space, which the digit-only font lacks, so it is left out. */
const time_text = computed(() => format_time(now.value, uses_24_hour_clock.value ? "HH:mm" : "h:mm"));

// ============================================================================
// Screen
// ============================================================================

function CasioFace() {
  return (
    <Screen>
      <Row x={60} y={0} width={180} height={20} justify="end" gap={5}>
        {unread_notifications.value > 0 && <Icon icon={icons.notification} size={18} color={TEXT_COLOR} />}
        {is_phone_connected.value && <Icon icon={icons.bluetooth} size={18} color={TEXT_COLOR} />}
        <BatteryIcon color={TEXT_COLOR} single_color />
        <Label font="small" color={TEXT_COLOR}>{`${battery_percent.value}%`}</Label>
      </Row>
      <Line points={[{ x: 107, y: 23 }, { x: 224, y: 23 }, { x: 229, y: 18 }]} color={TEXT_COLOR} />

      <Line
        points={[{ x: 0, y: 8 }, { x: 100, y: 8 }, { x: 95, y: 103 }, { x: 0, y: 103 }]}
        thickness={6}
        color={TEXT_COLOR}
      />
      <Label x={5} y={22} font="dot_matrix" color={TEXT_COLOR}>
        {`WK${pad_start(`${week_number_of(now.value, uses_24_hour_clock.value)}`, 2, "0")}`}
      </Label>
      <Label x={10} y={60} font="dot_matrix" color={TEXT_COLOR}>
        {to_upper(format_time(now.value, "ddd"))}
      </Label>

      <Label x={110} y={40} color={TEXT_COLOR}>
        {`${pad_start(`${day_number.value}`, 3)}-${pad_start(`${days_left_in_year.value}`, 3)}`}
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

      <HeartRateBadge
        x={5}
        y={214}
        height={24}
        color={TEXT_COLOR}
        icon_color={is_heart_rate_running.value ? TEXT_COLOR : HEART_OFF_COLOR}
      />
      <StepsBadge x={120} y={214} width={115} height={24} align="end" color={TEXT_COLOR} />
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
