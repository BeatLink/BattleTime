// Resistor clock watch face, ported from wasp-os's resistor_clock.
// Each digit of the time (hhmmss) and the date (ddmmyyyy) is a band in the resistor colour code.

import { computed } from "@preact/signals";
import {
  battery_percent,
  day_of,
  define_app,
  hour_of,
  minute_of,
  month_of,
  now,
  second_of,
  year_of,
  type Color,
} from "neotime";
import { StatusIcons } from "neotime/components";
import { icons } from "neotime/icons";
import { Column, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** Black, brown, red, orange, yellow, green, blue, violet, grey and white, for the digits 0 to 9. */
const BAND_COLORS: Color[] = [
  "#000000",
  "#9c6431",
  "#ff0000",
  "#ff9a00",
  "#ffff00",
  "#00ff00",
  "#0000ff",
  "#ff00ff",
  "#c0c0c0",
  "#ffffff",
];

// ============================================================================
// Derived values
// ============================================================================

/** Green when well charged, then yellow, then red, as wasp-os's battery meter. */
const battery_color = computed<Color>(() =>
  battery_percent.value > 50 ? "#00c000" : battery_percent.value > 20 ? "#e0c000" : "#e00000",
);

// ============================================================================
// Screen
// ============================================================================

function StatusBar() {
  return (
    <StatusIcons
      x={0}
      y={0}
      width={240}
      height={32}
      align="end"
      gap={6}
      padding={4}
      icon_size={22}
      color="accent"
      battery_color={battery_color.value}
    />
  );
}

/** A band for one decimal digit of `value`, `place` being 1 for units, 10 for tens and so on. */
function Band(props: { x: number; y: number; width: number; height: number; value: number; place: number }) {
  return (
    <Column
      x={props.x}
      y={props.y}
      width={props.width}
      height={props.height}
      background={BAND_COLORS[Math.floor(props.value / props.place) % 10]}
    />
  );
}

function TimeBand(props: { x: number; value: number; place: number }) {
  return <Band x={props.x} y={60} width={34} height={80} value={props.value} place={props.place} />;
}

function DateBand(props: { x: number; value: number; place: number }) {
  return <Band x={props.x} y={180} width={24} height={60} value={props.value} place={props.place} />;
}

function ResistorClockFace() {
  return (
    <Screen>
      <StatusBar />
      <TimeBand x={0} value={hour_of(now.value)} place={10} />
      <TimeBand x={40} value={hour_of(now.value)} place={1} />
      <TimeBand x={83} value={minute_of(now.value)} place={10} />
      <TimeBand x={123} value={minute_of(now.value)} place={1} />
      <TimeBand x={166} value={second_of(now.value)} place={10} />
      <TimeBand x={206} value={second_of(now.value)} place={1} />
      <DateBand x={0} value={day_of(now.value)} place={10} />
      <DateBand x={30} value={day_of(now.value)} place={1} />
      <DateBand x={63} value={month_of(now.value)} place={10} />
      <DateBand x={93} value={month_of(now.value)} place={1} />
      <DateBand x={126} value={year_of(now.value)} place={1000} />
      <DateBand x={156} value={year_of(now.value)} place={100} />
      <DateBand x={186} value={year_of(now.value)} place={10} />
      <DateBand x={216} value={year_of(now.value)} place={1} />
    </Screen>
  );
}

export default define_app({
  id: "resistor-clock",
  name: "Resistor clock",
  icon: icons.clock,
  kind: "face",
  screens: { main: ResistorClockFace },
});
