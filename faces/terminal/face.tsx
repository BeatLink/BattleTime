// Terminal watch face, ported from InfiniTime's WatchFaceTerminal.
// Each line is a white tag such as [TIME] followed by a coloured value, under a shell prompt.

import { computed } from "@preact/signals";
import {
  battery_percent,
  define_app,
  format_temperature,
  format_time,
  heart_rate,
  is_charging,
  is_phone_connected,
  now,
  persisted,
  steps_today,
  unread_notifications,
  weather,
  type Color,
  type WeatherCondition,
  type WeatherEvent,
} from "neotime";
import { icons } from "neotime/icons";
import { Column, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const LIGHT_GRAY: Color = "#b0b0b0";
const GRAY: Color = "#505050";
const GREEN: Color = "#11cc55";
const BLUE: Color = "#007fff";
const ORANGE: Color = "#ffb000";
const DEEP_ORANGE: Color = "#ff4000";
const YELLOW: Color = "#ffdd00";
const CONNECTED_BLUE: Color = "#0050ff";

const WEATHER_CONDITIONS: WeatherCondition[] = [
  "clear",
  "few_clouds",
  "clouds",
  "heavy_clouds",
  "rain",
  "showers",
  "thunderstorm",
  "snow",
  "mist",
];

/** InfiniTime's one-word name for each condition, in the order of `WEATHER_CONDITIONS`. */
const WEATHER_CONDITION_NAMES = ["Clear", "Clear", "Cloudy", "Cloudy", "Rain", "Showers", "Thunder", "Snow", "Mist"];

/** The latest temperature from the phone, in degrees Celsius. */
const weather_temperature = persisted("weather_temperature", 0);

/** The one-word condition of the latest forecast, or empty before any arrives. */
const weather_condition_name = persisted("weather_condition_name", "");

// ============================================================================
// Derived values
// ============================================================================

const time_text = computed(() =>
  format_time(now.value, "clock_with_seconds"),
);

const weather_text = computed(() =>
  weather_condition_name.value === ""
    ? "---"
    : `${format_temperature(weather_temperature.value)} ${weather_condition_name.value}`,
);

/** Red when empty through yellow to green when full, close to InfiniTime's battery colours. */
const battery_color = computed<Color>(() => {
  const percent = battery_percent.value;
  return percent < 5 ? "#ff2000" : percent < 12 ? "#ff8000" : percent < 20 ? "#ffd000" : percent < 35 ? "#c0ff00" : "#40ff00";
});

// ============================================================================
// Handlers
// ============================================================================

function store_weather(event: WeatherEvent) {
  weather_temperature.value = event.temperature;
  let place = 0;
  for (const condition of WEATHER_CONDITIONS) {
    if (condition === event.condition) weather_condition_name.value = WEATHER_CONDITION_NAMES[place];
    place = place + 1;
  }
}

// ============================================================================
// Screen
// ============================================================================

function TaggedLine(props: { tag: string; value: string; color: Color }) {
  return (
    <Row gap={12}>
      <Label color="#ffffff">{props.tag}</Label>
      <Label color={props.color}>{props.value}</Label>
    </Row>
  );
}

function TerminalFace() {
  const is_heart_rate_shown = heart_rate.value > 0;
  return (
    <Screen>
      <Column x={0} y={7} width={240}>
        <Label>{unread_notifications.value > 0 ? "[1]+ Notify" : ""}</Label>
        <Label color={LIGHT_GRAY}>user@watch:~ $ now</Label>
        <TaggedLine tag="[TIME]" value={time_text.value} color={GREEN} />
        <TaggedLine tag="[DATE]" value={format_time(now.value, "date")} color={BLUE} />
        <TaggedLine
          tag="[BATT]"
          value={is_charging.value ? `${battery_percent.value}% Charging` : `${battery_percent.value}%`}
          color={battery_color.value}
        />
        <TaggedLine tag="[STEP]" value={`${steps_today.value} steps`} color={ORANGE} />
        <TaggedLine
          tag="[L_HR]"
          value={is_heart_rate_shown ? `${heart_rate.value} bpm` : "---"}
          color={is_heart_rate_shown ? DEEP_ORANGE : GRAY}
        />
        <TaggedLine tag="[WTHR]" value={weather_text.value} color={YELLOW} />
        <TaggedLine
          tag="[STAT]"
          value={is_phone_connected.value ? "Connected" : "Disconnected"}
          color={is_phone_connected.value ? CONNECTED_BLUE : GRAY}
        />
        <Label color={LIGHT_GRAY}>user@watch:~ $</Label>
      </Column>
    </Screen>
  );
}

export default define_app({
  id: "terminal",
  name: "Terminal",
  icon: icons.clock,
  kind: "face",
  screens: { main: TerminalFace },
  subscriptions: [weather(store_weather)],
  permissions: ["background"],
});
