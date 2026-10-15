// RGB watch face, ported from ZephyrLabs' RGB (ZephyrLabs/Watchfaces).
// InfiniTime's classic digital layout whose time, date and icons cycle together round the colour wheel.
// The original shifts the hue a little many times a second; here it steps 30 degrees each second, the same lap of about 12 seconds.

import {
  computed,
  day_of,
  define_app,
  format_number,
  hour_of,
  is_phone_connected,
  minute_of,
  month_of,
  now,
  second_of,
  settings,
  unread_notifications,
  weekday_of,
  year_of,
  type Color,
} from "battletime";
import { display_hour } from "battletime/lib";
import { BatteryIcon, HeartRateBadge, StepsBadge } from "battletime/components";
import { icons } from "battletime/icons";
import { Icon, Label, Row, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

/** The colour wheel in 30 degree steps at the original's 85% saturation and 85% value, starting from red. */
const WHEEL_COLORS: Color[] = [
  "#d92121",
  "#d97d21",
  "#d9d921",
  "#7dd921",
  "#21d921",
  "#21d97d",
  "#21d9d9",
  "#217dd9",
  "#2121d9",
  "#7d21d9",
  "#d921d9",
  "#d9217d",
];

const WHEEL_STEPS = 12;

/** The notification icon stays grey, as in the original. */
const NOTIFICATION_COLOR: Color = "#666666";

/** Weekdays from Monday and months, in lower case as the original writes them. */
const DAY_NAMES = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

const MONTH_NAMES = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

// ============================================================================
// Derived values
// ============================================================================

/** The colour moves one step round the wheel every second. */
const wheel_color = computed(() => WHEEL_COLORS[second_of(now.value) % WHEEL_STEPS]);

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

/** A 12 hour clock leaves the hour's leading zero blank. */
const time_text = computed(() => {
  const hour = display_hour(now.value);
  const hour_text = uses_24_hour_clock.value ? format_number(hour, 2) : hour < 10 ? ` ${hour}` : `${hour}`;
  return `${hour_text}:${format_number(minute_of(now.value), 2)}`;
});

/** A 12 hour clock puts the month before the day. */
const date_text = computed(() => {
  const weekday = DAY_NAMES[weekday_of(now.value)];
  const month = MONTH_NAMES[month_of(now.value) - 1];
  const day = day_of(now.value);
  const year = year_of(now.value);
  return uses_24_hour_clock.value ? `${weekday} ${day} ${month} ${year}` : `${weekday} ${month} ${day} ${year}`;
});

// ============================================================================
// Screen
// ============================================================================

function RgbFace() {
  return (
    <Screen>
      {unread_notifications.value > 0 && (
        <Icon x={10} y={0} icon={icons.notification} size={20} color={NOTIFICATION_COLOR} />
      )}
      <Row x={120} y={4} width={115} height={20} justify="end" gap={5}>
        {is_phone_connected.value && <Icon icon={icons.bluetooth} size={20} color={wheel_color.value} />}
        <BatteryIcon color={wheel_color.value} />
      </Row>

      {!uses_24_hour_clock.value && (
        <Label x={196} y={50} font="small" color={wheel_color.value}>
          {hour_of(now.value) >= 12 ? "PM" : "AM"}
        </Label>
      )}
      <Label x={0} y={78} width={240} text_align="center" font="huge" color={wheel_color.value}>
        {time_text.value}
      </Label>

      <Label x={0} y={170} width={240} text_align="center" color={wheel_color.value}>
        {date_text.value}
      </Label>

      <HeartRateBadge x={5} y={214} height={24} color={wheel_color.value} />
      <StepsBadge x={120} y={214} width={115} height={24} align="end" color={wheel_color.value} />
    </Screen>
  );
}

export default define_app({
  id: "rgb",
  name: "RGB",
  icon: icons.clock,
  kind: "face",
  screens: { main: RgbFace },
});
