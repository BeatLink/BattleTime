// Current weather and forecast from the phone, merged from InfiniTime's Weather and wasp-os's weather app.
// It shows wasp-os's place name and InfiniTime's sky icon, condition, temperature coloured from
// freezing to hot, the day's low and high, and the coming days' forecast, in the user's unit.

import { computed } from "@preact/signals";
import {
  current_weather,
  define_app,
  format_temperature,
  format_time,
  now,
  persisted,
  range,
  settings,
  weather,
  DAY,
  HOUR,
  type Color,
  type WeatherCondition,
  type WeatherEvent,
} from "neotime";
import { icon_named, icons } from "neotime/icons";
import { Column, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** How many coming days the forecast holds. */
const FORECAST_LENGTH = 5;

/** How many coming days the phone sent, up to five; the system's `current_weather` holds today. */
const forecast_days = persisted("forecast_days", 0);

const forecast_conditions = persisted<WeatherCondition[]>("forecast_conditions", [
  "clear",
  "clear",
  "clear",
  "clear",
  "clear",
]);

const forecast_lows = persisted<number[]>("forecast_lows", [0, 0, 0, 0, 0]);

const forecast_highs = persisted<number[]>("forecast_highs", [0, 0, 0, 0, 0]);

// ============================================================================
// Derived values
// ============================================================================

const has_weather = computed(() => current_weather.value.received_at !== null);

/** When the phone last sent the weather, or 0 if it never has. */
const received_at = computed(() => current_weather.value.received_at ?? 0);

/** Degrees in the user's unit, rounded to the nearest, without the unit. */
function in_chosen_unit(celsius: number): number {
  return settings.uses_metric_units.value ? celsius : Math.floor((celsius * 18 + 5) / 10) + 32;
}

/** The colour InfiniTime gives a temperature, from blue when freezing to deep orange when hot. */
function color_of(celsius: number): Color {
  if (celsius <= 0) return "#0050ff";
  if (celsius <= 4) return "#00ffff";
  if (celsius >= 27) return "#ff4000";
  return "#ffb000";
}

const condition_text = computed(() => {
  const sky = current_weather.value.condition;
  if (sky === "clear") return "Clear sky";
  if (sky === "few_clouds") return "Few clouds";
  if (sky === "clouds") return "Scattered clouds";
  if (sky === "heavy_clouds") return "Broken clouds";
  if (sky === "showers") return "Shower rain";
  if (sky === "rain") return "Rain";
  if (sky === "thunderstorm") return "Thunderstorm";
  if (sky === "snow") return "Snow";
  return "Mist";
});

/** Weather more than a few hours old is shown dimmed, as it may no longer be true. */
const is_stale = computed(() => now.value - received_at.value > 3 * HOUR);

// ============================================================================
// Handlers
// ============================================================================

/** The coming days arrive only with the forecast event, so they are kept here. */
function receive(event: WeatherEvent) {
  const day_count = Math.min(event.days.length, FORECAST_LENGTH);
  forecast_days.value = day_count;
  for (const place of range(FORECAST_LENGTH)) {
    if (place < day_count) {
      forecast_conditions.value[place] = event.days[place].condition;
      forecast_lows.value[place] = event.days[place].low;
      forecast_highs.value[place] = event.days[place].high;
    }
  }
}

// ============================================================================
// Screen
// ============================================================================

function ForecastDay(props: { place: number }) {
  return (
    <>
      {props.place < forecast_days.value && (
        <Column align_items="center" width={46}>
          <Label font="small" color="#b0b0b0">{format_time(now.value + (props.place + 1) * DAY, "ddd")}</Label>
          <Icon icon={icon_named(forecast_conditions.value[props.place])} size={24} />
          <Label font="small" color={color_of(forecast_highs.value[props.place])}>
            {`${in_chosen_unit(forecast_highs.value[props.place])}`}
          </Label>
          <Label font="small" color={color_of(forecast_lows.value[props.place])}>
            {`${in_chosen_unit(forecast_lows.value[props.place])}`}
          </Label>
        </Column>
      )}
    </>
  );
}

function Weather() {
  return (
    <Screen
      padding={4}
      align_items="center"
      justify={has_weather.value ? "start" : "center"}
      gap={has_weather.value ? 0 : 12}
    >
      {!has_weather.value && <Icon icon={icons.weather} size={64} color="muted" />}
      {!has_weather.value && <Label color="muted" text_align="center">No weather data.</Label>}
      {has_weather.value && (
        <Row justify="space_between" width={232}>
          <Label font="small" color="muted" long_mode="scroll" grow={1}>{current_weather.value.location}</Label>
          <Label font="small" color="muted">{format_time(received_at.value, "clock")}</Label>
        </Row>
      )}
      {has_weather.value && (
        <Icon icon={icon_named(current_weather.value.condition)} size={44} color={is_stale.value ? "muted" : "text"} />
      )}
      {has_weather.value && <Label font="small" color="#b0b0b0">{condition_text.value}</Label>}
      {has_weather.value && (
        <Row gap={10} justify="center">
          <Label color={color_of(current_weather.value.low)}>{`${in_chosen_unit(current_weather.value.low)}°`}</Label>
          <Label font="large" color={is_stale.value ? "muted" : color_of(current_weather.value.temperature)}>
            {format_temperature(current_weather.value.temperature)}
          </Label>
          <Label color={color_of(current_weather.value.high)}>{`${in_chosen_unit(current_weather.value.high)}°`}</Label>
        </Row>
      )}
      {has_weather.value && (
        <Row grow={1} align_items="end" justify="space_between" width={232}>
          {range(FORECAST_LENGTH).map((place) => (
            <ForecastDay place={place} />
          ))}
        </Row>
      )}
    </Screen>
  );
}

export default define_app({
  id: "weather",
  name: "Weather",
  icon: icons.weather,
  screens: { main: Weather },
  subscriptions: [weather(receive)],
  permissions: ["background"],
});
