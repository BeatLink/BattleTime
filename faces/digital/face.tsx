// Digital watch face, merged from InfiniTime's WatchFaceDigital and wasp-os's clock.
// InfiniTime gives the layout: status icons, weather, date by clock type, heart rate and steps.
// wasp-os gives the two-tone digits, with the tens dimmer than the units, and no leading zero on a 12 hour clock.

import { computed } from "@preact/signals";
import {
  battery_percent,
  define_app,
  format_temperature,
  format_time,
  heart_rate,
  hour_of,
  is_charging,
  is_phone_connected,
  minute_of,
  now,
  persisted,
  settings,
  steps_today,
  unread_notifications,
  weather,
  type Color,
  type WeatherCondition,
  type WeatherEvent,
} from "neotime";
import { icon_named, icons } from "neotime/icons";
import { Column, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const WEATHER_COLOR: Color = "#999999";
const HEART_COLOR: Color = "#ce1b1b";
const HEART_OFF_COLOR: Color = "#1b1b1b";
const STEPS_COLOR: Color = "#00ffe7";

/** The latest temperature from the phone, in degrees Celsius. */
const weather_temperature = persisted("weather_temperature", 0);

/** The latest condition from the phone, or "none" before any forecast arrives. */
const weather_condition = persisted<WeatherCondition | "none">("weather_condition", "none");

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

const hour_units = computed(() => `${display_hour.value % 10}`);

const minute_tens = computed(() => `${Math.floor(minute_of(now.value) / 10)}`);

const minute_units = computed(() => `${minute_of(now.value) % 10}`);

const date_text = computed(() =>
  format_time(now.value, uses_24_hour_clock.value ? "ddd D MMM YYYY" : "ddd MMM D YYYY"),
);

const has_weather = computed(() => weather_condition.value !== "none");

const is_heart_rate_shown = computed(() => heart_rate.value > 0);

// ============================================================================
// Handlers
// ============================================================================

function store_weather(event: WeatherEvent) {
  weather_temperature.value = event.temperature;
  weather_condition.value = event.condition;
}

// ============================================================================
// Screen
// ============================================================================

function BatteryLevel() {
  return (
    <Row width={26} height={14} border_width={2} border_color="text" radius={2} padding={1}>
      <Column width={Math.max(1, Math.floor((battery_percent.value * 20) / 100))} height={8} background="text" />
    </Row>
  );
}

function DigitalFace() {
  return (
    <Screen>
      {unread_notifications.value > 0 && <Icon x={0} y={0} icon={icons.notification} size={24} color="#00ff00" />}

      <Row x={120} y={0} width={120} height={24} justify="end" gap={6}>
        {is_phone_connected.value && <Icon icon={icons.bluetooth} size={20} />}
        {is_charging.value && <Icon icon={icons.charging} size={20} />}
        <BatteryLevel />
      </Row>

      {has_weather.value && (
        <Row x={0} y={44} width={240} justify="center" gap={8}>
          <Icon icon={icon_named(weather_condition.value as WeatherCondition)} size={24} color={WEATHER_COLOR} />
          <Label color={WEATHER_COLOR}>
            {format_temperature(weather_temperature.value)}
          </Label>
        </Row>
      )}

      {!uses_24_hour_clock.value && (
        <Label x={196} y={66} font="small" color="muted">
          {hour_of(now.value) >= 12 ? "PM" : "AM"}
        </Label>
      )}

      <Row x={0} y={80} width={240} justify="center">
        <Label font="huge" color="muted">{hour_tens.value}</Label>
        <Label font="huge">{hour_units.value}</Label>
        <Label font="huge" color="muted">:</Label>
        <Label font="huge" color="muted">{minute_tens.value}</Label>
        <Label font="huge">{minute_units.value}</Label>
      </Row>

      <Label x={0} y={169} width={240} text_align="center" color={WEATHER_COLOR}>
        {date_text.value}
      </Label>

      <Row x={0} y={212} height={28} gap={5}>
        <Icon icon={icons.heart} size={24} color={is_heart_rate_shown.value ? HEART_COLOR : HEART_OFF_COLOR} />
        {is_heart_rate_shown.value && <Label color={HEART_COLOR}>{`${heart_rate.value}`}</Label>}
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
  subscriptions: [weather(store_weather)],
  permissions: ["background"],
});
