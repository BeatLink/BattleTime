// Digital clock with the weekday, ported from wasp-os's week_clock, which extends its clock face.
// Big two-tone digits, the tens dimmer than the units, over the weekday and date, with wasp-os's status bar above.

import { computed } from "@preact/signals";
import {
  battery_percent,
  define_app,
  format_time,
  hour_of,
  is_charging,
  is_phone_connected,
  minute_of,
  now,
  settings,
  unread_notifications,
  type Color,
} from "neotime";
import { icons } from "neotime/icons";
import { Column, Icon, Label, Row, Screen } from "neotime/widgets";

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
    <Row x={0} y={0} width={240} height={32} justify="end" gap={6} padding={4}>
      {unread_notifications.value > 0 && <Icon icon={icons.notification} size={22} color="accent" />}
      {is_phone_connected.value && <Icon icon={icons.bluetooth} size={22} color="accent" />}
      {is_charging.value ? (
        <Icon icon={icons.charging} size={22} color="warning" />
      ) : (
        <Row width={26} height={14} border_width={2} border_color={battery_color.value} radius={2} padding={1}>
          <Column
            width={Math.max(1, Math.floor((battery_percent.value * 20) / 100))}
            height={8}
            background={battery_color.value}
          />
        </Row>
      )}
    </Row>
  );
}

function WeekClockFace() {
  return (
    <Screen>
      <StatusBar />
      {!uses_24_hour_clock.value && (
        <Label x={192} y={52} width={48} text_align="center" color="muted">
          {hour_of(now.value) >= 12 ? "PM" : "AM"}
        </Label>
      )}
      <Row x={0} y={80} width={240} justify="center">
        <Label font="huge" color="muted">{hour_tens.value}</Label>
        <Label font="huge">{`${display_hour.value % 10}`}</Label>
        <Label font="huge" color="muted">:</Label>
        <Label font="huge" color="muted">{`${Math.floor(minute_of(now.value) / 10)}`}</Label>
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
