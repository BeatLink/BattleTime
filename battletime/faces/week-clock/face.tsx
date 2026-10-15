// Digital clock with the weekday, ported from wasp-os's week_clock, which extends its clock face.
// Big two-tone digits, the tens dimmer than the units, over the weekday and date, with wasp-os's status bar above.

import {
  battery_percent,
  computed,
  define_app,
  format_time,
  hour_of,
  minute_of,
  now,
  settings,
  type Color,
} from "battletime";
import { StatusIcons } from "battletime/components";
import { icons } from "battletime/icons";
import { Label, Row, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

/** wasp-os draws the tens in its theme's mid shade, about this much of the bright one on black. */
const TENS_OPACITY = 185;

/** wasp-os lightens the mid shade one step for the colon and the AM or PM. */
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

function WeekClockFace() {
  return (
    <Screen>
      <StatusBar />
      {!uses_24_hour_clock.value && (
        <Label x={192} y={52} width={48} text_align="center" opacity={COLON_OPACITY}>
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
      <Label x={0} y={180} width={240} text_align="center">
        {format_time(now.value, "ddd D MMM YYYY")}
      </Label>
    </Screen>
  );
}

export default define_app({
  id: "week-clock",
  name: "Week clock",
  icon: icons.clock,
  kind: "face",
  screens: { main: WeekClockFace },
});
