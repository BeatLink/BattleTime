// PineTimeStyle watch face, ported from InfiniTime's WatchFacePineTimeStyle, itself styled after TimeStyle for Pebble.
// A coloured time panel of giant digits on the left and a sidebar with status icons, weather, a calendar and a step gauge with a needle.
// InfiniTime's pop-up colour and option buttons are a settings screen here, opened by a long press.

import { computed } from "@preact/signals";
import {
  back,
  current_weather,
  day_of,
  define_app,
  format_number,
  format_time,
  go,
  hour_of,
  is_charging,
  is_phone_connected,
  minute_of,
  now,
  persisted,
  random_int,
  settings,
  steps_today,
  to_upper,
  unread_notifications,
  type Color,
  type Instant,
} from "neotime";
import { BatteryIcon } from "neotime/components";
import { icon_named, icons } from "neotime/icons";
import { Button, Column, Icon, Label, Row, Scale, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** InfiniTime's colour choices, in its order. */
const COLORS: Color[] = [
  "#ffffff",
  "#c0c0c0",
  "#808080",
  "#000000",
  "#ff0000",
  "#b00000",
  "#ffff00",
  "#b0b000",
  "#00ff00",
  "#00b000",
  "#00ffff",
  "#00b0b0",
  "#0000ff",
  "#0000b0",
  "#ff00ff",
  "#b000b0",
  "#ffa500",
  "#ffaec9",
];

const COLOR_NAMES = [
  "White",
  "Silver",
  "Gray",
  "Black",
  "Red",
  "Maroon",
  "Yellow",
  "Olive",
  "Lime",
  "Green",
  "Cyan",
  "Teal",
  "Blue",
  "Navy",
  "Magenta",
  "Purple",
  "Orange",
  "Pink",
];

const COLOR_COUNT = 18;
const WHITE = 0;
const BLACK = 3;
const TEAL = 11;

const GAUGE_STYLE_FULL = 0;
const GAUGE_STYLE_HALF = 1;
const GAUGE_STYLE_NUMERIC = 2;
const GAUGE_STYLE_NAMES = ["Full", "Half", "Numeric"];

/** Places in `COLORS` for the time digits, the sidebar and the time panel. */
const time_color = persisted("time_color", TEAL);
const bar_color = persisted("bar_color", TEAL);
const background_color = persisted("background_color", BLACK);

/** How the sidebar shows steps: a full gauge, a half gauge with seconds, or a number. */
const gauge_style = persisted("gauge_style", GAUGE_STYLE_FULL);

const shows_weather = persisted("shows_weather", true);

// ============================================================================
// Derived values
// ============================================================================

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

const display_hour = computed(() => {
  const hour = hour_of(now.value);
  return uses_24_hour_clock.value ? hour : hour % 12 === 0 ? 12 : hour % 12;
});

/** The steps as a share of the goal, wrapping past 100 as InfiniTime's gauge does. */
export const step_gauge_value = computed(
  () => Math.floor(steps_today.value / Math.max(1, Math.floor(settings.step_goal.value / 100))) % 100,
);

/** The needle is black on a white sidebar and white on any other. */
const needle_color = computed<Color>(() => (bar_color.value === WHITE ? "#000000" : "#ffffff"));

/** The gauge's marks are black until the step goal is passed, then white. */
const gauge_mark_color = computed<Color>(() =>
  steps_today.value > settings.step_goal.value ? "#ffffff" : "#000000",
);

const has_weather = computed(() => current_weather.value.received_at !== null);

const temperature_text = computed(() => {
  const celsius = current_weather.value.temperature;
  if (!has_weather.value) return "--";
  return `${settings.uses_metric_units.value ? celsius : Math.round((celsius * 9) / 5) + 32}°`;
});

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

/** The condition's icon, the moon in place of the sun at night, and a cross before any forecast, as InfiniTime does. */
const weather_icon = computed(() => {
  const condition = current_weather.value.condition;
  if (!has_weather.value) return icons.close;
  if (is_night.value && condition === "clear") return icons.weather_night_clear;
  if (is_night.value && condition === "few_clouds") return icons.weather_night_cloudy;
  return icon_named(condition);
});

/** The calendar sits lower when the weather is shown above it. */
const calendar_top = computed(() => (shows_weather.value ? 123 : 103));

// ============================================================================
// Handlers
// ============================================================================

function next_color(place: number): number {
  return place + 1 < COLOR_COUNT ? place + 1 : 0;
}

function previous_color(place: number): number {
  return place > 0 ? place - 1 : COLOR_COUNT - 1;
}

function change_time_color(is_forward: boolean) {
  let place = is_forward ? next_color(time_color.value) : previous_color(time_color.value);
  if (place === background_color.value) place = is_forward ? next_color(place) : previous_color(place);
  time_color.value = place;
}

function change_bar_color(is_forward: boolean) {
  let place = is_forward ? next_color(bar_color.value) : previous_color(bar_color.value);
  if (place === BLACK) place = is_forward ? next_color(place) : previous_color(place);
  bar_color.value = place;
}

function change_background_color(is_forward: boolean) {
  let place = is_forward ? next_color(background_color.value) : previous_color(background_color.value);
  if (place === time_color.value) place = is_forward ? next_color(place) : previous_color(place);
  background_color.value = place;
}

function reset_colors() {
  time_color.value = TEAL;
  bar_color.value = TEAL;
  background_color.value = BLACK;
}

function randomise_colors() {
  const time = random_int(0, COLOR_COUNT - 1);
  let bar = random_int(0, COLOR_COUNT - 1);
  let background = random_int(0, COLOR_COUNT - 1);
  if (time === background) background = next_color(background);
  if (bar === BLACK) bar = previous_color(bar);
  time_color.value = time;
  bar_color.value = bar;
  background_color.value = background;
}

function next_gauge_style() {
  gauge_style.value = (gauge_style.value + 1) % 3;
}

function toggle_weather() {
  shows_weather.value = !shows_weather.value;
}

// ============================================================================
// Screens
// ============================================================================

function Calendar() {
  const top = calendar_top.value;
  return (
    <>
      <Label x={200} y={top - 24} width={40} text_align="center" font="small" color="#000000">
        {to_upper(format_time(now.value, "ddd"))}
      </Label>
      <Column x={203} y={top} width={34} height={34} background="#000000" />
      <Column x={206} y={top + 3} width={28} height={28} background="#ffffff" />
      <Column x={213} y={top - 3} width={3} height={12} background="#000000" />
      <Column x={224} y={top - 3} width={3} height={12} background="#000000" />
      <Label x={203} y={top + 8} width={34} text_align="center" font="small" color="#000000">
        {`${day_of(now.value)}`}
      </Label>
      <Label x={200} y={top + 38} width={40} text_align="center" font="small" color="#000000">
        {to_upper(format_time(now.value, "MMM"))}
      </Label>
    </>
  );
}

/** InfiniTime's needle gauges, the half one over the top from nine to three o'clock; minor ticks match the sidebar so only the marks show. */
function StepGauge() {
  return (
    <>
      {gauge_style.value === GAUGE_STYLE_FULL && (
        <Scale
          x={200}
          y={200}
          diameter={40}
          ticks={100}
          major_every={10}
          color={COLORS[bar_color.value]}
          major_color={gauge_mark_color.value}
          value={step_gauge_value.value}
          needle_color={needle_color.value}
        />
      )}
      {gauge_style.value === GAUGE_STYLE_HALF && (
        <Scale
          x={201}
          y={186}
          diameter={37}
          ticks={101}
          major_every={25}
          start_angle={270}
          end_angle={90}
          color={COLORS[bar_color.value]}
          major_color={gauge_mark_color.value}
          value={step_gauge_value.value}
          needle_color={needle_color.value}
        />
      )}
      {gauge_style.value === GAUGE_STYLE_HALF && (
        <Label x={200} y={218} width={40} text_align="center" font="small" color="#000000">
          {format_time(now.value, "[:]ss")}
        </Label>
      )}
      {gauge_style.value === GAUGE_STYLE_NUMERIC && (
        <Icon x={208} y={192} icon={icons.steps} size={24} color="#000000" />
      )}
      {gauge_style.value === GAUGE_STYLE_NUMERIC && (
        <Label x={200} y={218} width={40} text_align="center" font="small" color="#000000">
          {`${Math.floor(steps_today.value / 1000)}K`}
        </Label>
      )}
    </>
  );
}

function PineTimeStyleFace() {
  const digit_color = COLORS[time_color.value];
  return (
    <Screen background={COLORS[background_color.value]} onLongPress={() => go("settings")}>
      {unread_notifications.value > 0 && (
        <Icon x={5} y={5} icon={icons.notification} size={20} color={digit_color} />
      )}
      <Label x={5} y={0} width={200} text_align="center" font="giant" color={digit_color}>
        {format_number(display_hour.value, 2)}
      </Label>
      <Label x={5} y={110} width={200} text_align="center" font="giant" color={digit_color}>
        {format_number(minute_of(now.value), 2)}
      </Label>
      {!uses_24_hour_clock.value && (
        <Label x={2} y={172} font="small" color={digit_color}>
          {hour_of(now.value) >= 12 ? "P\nM" : "A\nM"}
        </Label>
      )}

      <Column x={200} y={0} width={40} height={240} background={COLORS[bar_color.value]} />
      {is_phone_connected.value && <Icon x={201} y={2} icon={icons.bluetooth} size={16} color="#000000" />}
      {is_charging.value ? (
        <Icon x={219} y={2} icon={icons.charging} size={16} color="#000000" />
      ) : (
        <Column x={208} y={2}>
          <BatteryIcon color="#000000" single_color />
        </Column>
      )}

      {shows_weather.value && (
        <Icon
          x={206}
          y={32}
          icon={weather_icon.value}
          size={28}
          color="#000000"
        />
      )}
      {shows_weather.value && (
        <Label x={200} y={64} width={40} text_align="center" font="small" color="#000000">
          {temperature_text.value}
        </Label>
      )}

      <Calendar />
      <StepGauge />
    </Screen>
  );
}

function ColorChooser(props: { title: string; place: number; onChange: (is_forward: boolean) => void }) {
  return (
    <Row gap={6} height={40}>
      <Button width={48} height="100%" onClick={() => props.onChange(false)}>
        {"<"}
      </Button>
      <Column grow={1} align_items="center">
        <Label font="small" color="muted">{props.title}</Label>
        <Label font="small" color={COLORS[props.place]}>{COLOR_NAMES[props.place]}</Label>
      </Column>
      <Button width={48} height="100%" onClick={() => props.onChange(true)}>
        {">"}
      </Button>
    </Row>
  );
}

function Settings() {
  return (
    <Screen padding={4} gap={4}>
      <ColorChooser title="Time" place={time_color.value} onChange={change_time_color} />
      <ColorChooser title="Bar" place={bar_color.value} onChange={change_bar_color} />
      <ColorChooser title="Background" place={background_color.value} onChange={change_background_color} />
      <Row gap={4} height={40}>
        <Button grow={1} height="100%" font="small" onClick={next_gauge_style}>
          {`Steps: ${GAUGE_STYLE_NAMES[gauge_style.value]}`}
        </Button>
        <Button grow={1} height="100%" font="small" onClick={toggle_weather}>
          {shows_weather.value ? "Weather: On" : "Weather: Off"}
        </Button>
      </Row>
      <Row gap={4} height={40}>
        <Button grow={1} height="100%" onClick={reset_colors}>
          Reset
        </Button>
        <Button grow={1} height="100%" onClick={randomise_colors}>
          Random
        </Button>
        <Button grow={1} height="100%" background="accent" onClick={back}>
          <Icon icon={icons.check} size={24} />
        </Button>
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "pinetime-style",
  name: "PineTimeStyle",
  icon: icons.clock,
  kind: "face",
  screens: { main: PineTimeStyleFace, settings: Settings },
});
