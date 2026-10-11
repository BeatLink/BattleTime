// Lite weather watch face, ported from PinetimeLite's WatchFaceWeather.
// Big yellow hour digits sit over the minute digits, with the seconds beside them and the weather icon top right.
// The short weekday in green and the day of the month in red sit along the bottom.

import { computed } from "@preact/signals";
import {
  current_weather,
  day_of,
  define_app,
  format_number,
  format_time,
  hour_of,
  minute_of,
  now,
  second_of,
  settings,
  to_upper,
  type Color,
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

// ============================================================================
// Derived values
// ============================================================================

const hour_text = computed(() => {
  const hour = hour_of(now.value);
  return format_number(settings.uses_24_hour_clock.value ? hour : hour % 12 === 0 ? 12 : hour % 12, 2);
});

/** The icon shows once the phone has sent a forecast. */
const has_weather = computed(() => current_weather.value.received_at !== null);

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
      <Label x={160} y={110} font="condensed" color={DIGIT_COLOR}>
        {format_number(second_of(now.value), 2)}
      </Label>

      {has_weather.value && (
        <Icon x={150} y={14} icon={icon_named(current_weather.value.condition)} size={64} color={WEATHER_COLOR} />
      )}

      <Row x={10} y={190} gap={12} align_items="center">
        <Label font="large" color={WEEKDAY_COLOR}>{to_upper(format_time(now.value, "ddd"))}</Label>
        <Label font="condensed" color={DAY_COLOR}>{`${day_of(now.value)}`}</Label>
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
});
