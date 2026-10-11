// Lite weather watch face, ported from PinetimeLite's WatchFaceWeather.
// Big yellow hour digits sit over the minute digits, with the seconds beside them and the weather icon top right.
// The short weekday in green and the day of the month in red sit along the bottom.

import { computed } from "@preact/signals";
import {
  day_of,
  define_app,
  format_number,
  format_time,
  hour_of,
  minute_of,
  now,
  persisted,
  second_of,
  settings,
  weather,
  type Color,
  type WeatherCondition,
  type WeatherEvent,
} from "neotime";
import { icon_named, icons } from "neotime/icons";
import { Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const DIGIT_COLOR: Color = "#ffcc00";
const WEATHER_COLOR: Color = "#ffaa00";
const WEEKDAY_COLOR: Color = "#225500";
const DAY_COLOR: Color = "#ce1b1b";

/** The latest condition from the phone, or "none" before any forecast arrives. */
const weather_condition = persisted<WeatherCondition | "none">("weather_condition", "none");

// ============================================================================
// Derived values
// ============================================================================

const hour_text = computed(() => {
  const hour = hour_of(now.value);
  return format_number(settings.uses_24_hour_clock.value ? hour : hour % 12 === 0 ? 12 : hour % 12, 2);
});

// ============================================================================
// Handlers
// ============================================================================

function store_weather(event: WeatherEvent) {
  weather_condition.value = event.condition;
}

// ============================================================================
// Screen
// ============================================================================

function LiteWeatherFace() {
  return (
    <Screen>
      <Label x={10} y={0} font="huge" color={DIGIT_COLOR}>
        {hour_text.value}
      </Label>
      <Label x={10} y={92} font="huge" color={DIGIT_COLOR}>
        {format_number(minute_of(now.value), 2)}
      </Label>
      <Label x={150} y={120} font="large" color={DIGIT_COLOR}>
        {format_number(second_of(now.value), 2)}
      </Label>

      {weather_condition.value !== "none" && (
        <Icon x={150} y={14} icon={icon_named(weather_condition.value)} size={64} color={WEATHER_COLOR} />
      )}

      <Row x={10} y={196} gap={12}>
        <Label font="large" color={WEEKDAY_COLOR}>{format_time(now.value, "ddd").toUpperCase()}</Label>
        <Label font="large" color={DAY_COLOR}>{`${day_of(now.value)}`}</Label>
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "lite-weather",
  name: "Lite Weather",
  icon: icons.weather,
  kind: "face",
  screens: { main: LiteWeatherFace },
  subscriptions: [weather(store_weather)],
  permissions: ["background"],
});
