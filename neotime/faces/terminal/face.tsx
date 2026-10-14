// Terminal watch face, ported from InfiniTime's WatchFaceTerminal.
// Each line is a white tag such as [TIME] followed by a coloured value, under a shell prompt.

import {
  battery_percent,
  computed,
  current_weather,
  define_app,
  format_temperature,
  format_time,
  heart_rate,
  is_bluetooth_enabled,
  is_charging,
  is_heart_rate_running,
  is_phone_connected,
  now,
  settings,
  steps_today,
  unread_notifications,
  type Color,
} from "neotime";
import { weather_condition_name } from "neotime/lib";
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


// ============================================================================
// Derived values
// ============================================================================

/** InfiniTime always shows two hour digits, with AM or PM after them on a 12 hour clock. */
const time_text = computed(() =>
  format_time(now.value, settings.uses_24_hour_clock.value ? "HH:mm:ss" : "hh:mm:ss A"),
);

/** InfiniTime's one-word name for the condition. */
const condition_name = computed(() => weather_condition_name(current_weather.value.condition));

const weather_text = computed(() => {
  if (current_weather.value.received_at === null) return "---";
  return `${format_temperature(current_weather.value.temperature)} ${condition_name.value}`;
});

/** Red when empty through yellow to green when full, close to InfiniTime's battery colours. */
const battery_color = computed<Color>(() => {
  const percent = battery_percent.value;
  return percent < 5 ? "#ff2000" : percent < 12 ? "#ff8000" : percent < 20 ? "#ffd000" : percent < 35 ? "#c0ff00" : "#40ff00";
});

/** Disabled when Bluetooth is off, otherwise whether the phone is connected. */
const status_text = computed(() =>
  !is_bluetooth_enabled.value ? "Disabled" : is_phone_connected.value ? "Connected" : "Disconnected",
);

// ============================================================================
// Screen
// ============================================================================

function TaggedLine(props: { tag: string; value: string; color: Color }) {
  return (
    <Row gap={12}>
      <Label font="small" color="#ffffff">{props.tag}</Label>
      <Label font="small" color={props.color}>{props.value}</Label>
    </Row>
  );
}

function TerminalFace() {
  return (
    <Screen>
      <Column x={0} y={7} width={240}>
        <Label font="small">{unread_notifications.value > 0 ? "[1]+ Notify" : ""}</Label>
        <Label font="small" color={LIGHT_GRAY}>user@watch:~ $ now</Label>
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
          value={is_heart_rate_running.value ? `${heart_rate.value} bpm` : "---"}
          color={is_heart_rate_running.value ? DEEP_ORANGE : GRAY}
        />
        <TaggedLine tag="[WTHR]" value={weather_text.value} color={YELLOW} />
        <TaggedLine
          tag="[STAT]"
          value={status_text.value}
          color={is_bluetooth_enabled.value && is_phone_connected.value ? CONNECTED_BLUE : GRAY}
        />
        <Label font="small" color={LIGHT_GRAY}>user@watch:~ $</Label>
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
});
