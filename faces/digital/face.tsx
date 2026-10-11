// Digital watch face, merged from InfiniTime's WatchFaceDigital and wasp-os's clock.
// InfiniTime gives the layout: status icons with the alarm bell, weather, date by clock type, heart rate and steps.
// wasp-os gives the two-tone digits, with the tens dimmer than the units, and no leading zero on a 12 hour clock.

import { computed } from "@preact/signals";
import {
  current_weather,
  define_app,
  format_temperature,
  format_time,
  heart_rate,
  hour_of,
  is_heart_rate_running,
  is_phone_connected,
  minute_of,
  next_alarm,
  now,
  settings,
  steps_today,
  unread_notifications,
  type Color,
  type Instant,
} from "neotime";
import { BatteryIcon } from "neotime/components";
import { icon_named, icons } from "neotime/icons";
import { Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const WEATHER_COLOR: Color = "#999999";
const HEART_COLOR: Color = "#ce1b1b";
const HEART_OFF_COLOR: Color = "#1b1b1b";
const STEPS_COLOR: Color = "#00ffe7";

/** wasp-os draws the tens in its theme's mid shade, about this much of the bright one on black. */
const TENS_OPACITY = 185;

/** wasp-os lightens the mid shade one step for the colon. */
const COLON_OPACITY = 195;

// ============================================================================
// Derived values
// ============================================================================

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

const display_hour = computed(() => {
  const hour = hour_of(now.value);
  return uses_24_hour_clock.value ? hour : hour % 12 === 0 ? 12 : hour % 12;
});

/** A 12 hour clock shows 9:30, not 09:30, so the tens place holds a space. */
const hour_tens = computed(() =>
  uses_24_hour_clock.value || display_hour.value >= 10 ? `${Math.floor(display_hour.value / 10)}` : " ",
);

const date_text = computed(() =>
  format_time(now.value, uses_24_hour_clock.value ? "ddd D MMM YYYY" : "ddd MMM D YYYY"),
);

const has_weather = computed(() => current_weather.value.received_at !== null);

function minute_of_day(instant: Instant): number {
  return hour_of(instant) * 60 + minute_of(instant);
}

/** Night runs from sunset to sunrise, or from 18:00 to 06:00 until the phone sends them. */
const is_night = computed(() => {
  if (current_weather.value.sunrise === null || current_weather.value.sunset === null) {
    return hour_of(now.value) < 6 || hour_of(now.value) >= 18;
  }
  return (
    minute_of_day(now.value) < minute_of_day(current_weather.value.sunrise) ||
    minute_of_day(now.value) >= minute_of_day(current_weather.value.sunset)
  );
});

/** The condition's icon, with the moon in place of the sun at night as InfiniTime does. */
const weather_icon = computed(() => {
  const condition = current_weather.value.condition;
  if (is_night.value && condition === "clear") return icons.weather_night_clear;
  if (is_night.value && condition === "few_clouds") return icons.weather_night_cloudy;
  return icon_named(condition);
});

// ============================================================================
// Screen
// ============================================================================

function DigitalFace() {
  return (
    <Screen>
      {unread_notifications.value > 0 && <Icon x={0} y={0} icon={icons.notification} size={24} color="#00ff00" />}

      <Row x={120} y={0} width={120} height={24} justify="end" gap={6}>
        {is_phone_connected.value && <Icon icon={icons.bluetooth} size={20} />}
        {next_alarm.value !== null && <Icon icon={icons.bell} size={20} />}
        <BatteryIcon />
      </Row>

      {has_weather.value && (
        <Row x={0} y={44} width={240} justify="center" gap={8}>
          <Icon icon={weather_icon.value} size={24} color={WEATHER_COLOR} />
          <Label color={WEATHER_COLOR}>{format_temperature(current_weather.value.temperature)}</Label>
        </Row>
      )}

      {!uses_24_hour_clock.value && (
        <Label x={196} y={66} font="small" opacity={COLON_OPACITY}>
          {format_time(now.value, "A")}
        </Label>
      )}

      <Row x={0} y={80} width={240} justify="center">
        <Label font="huge" opacity={TENS_OPACITY}>{hour_tens.value}</Label>
        <Label font="huge">{`${display_hour.value % 10}`}</Label>
        <Label font="huge" opacity={COLON_OPACITY}>:</Label>
        <Label font="huge" opacity={TENS_OPACITY}>{`${Math.floor(minute_of(now.value) / 10)}`}</Label>
        <Label font="huge">{`${minute_of(now.value) % 10}`}</Label>
      </Row>

      <Label x={0} y={169} width={240} text_align="center" color={WEATHER_COLOR}>
        {date_text.value}
      </Label>

      <Row x={0} y={212} height={28} gap={5}>
        <Icon icon={icons.heart} size={24} color={is_heart_rate_running.value ? HEART_COLOR : HEART_OFF_COLOR} />
        {is_heart_rate_running.value && <Label color={HEART_COLOR}>{`${heart_rate.value}`}</Label>}
      </Row>

      <Row x={120} y={212} width={120} height={28} justify="end" gap={5}>
        <Icon icon={icons.steps} size={24} color={STEPS_COLOR} />
        <Label color={STEPS_COLOR}>{`${steps_today.value}`}</Label>
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "digital",
  name: "Digital",
  icon: icons.clock,
  kind: "face",
  screens: { main: DigitalFace },
});
