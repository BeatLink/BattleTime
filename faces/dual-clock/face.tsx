// Dual clock watch face, ported from wasp-os's dual_clock.
// The hours sit above the minutes in big digits, hours dimmer than minutes, with wasp-os's status bar above.

import { computed } from "@preact/signals";
import {
  battery_percent,
  define_app,
  format_number,
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

const display_hour = computed(() => {
  const hour = hour_of(now.value);
  return settings.uses_24_hour_clock.value ? hour : hour % 12 === 0 ? 12 : hour % 12;
});

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

function DualClockFace() {
  return (
    <Screen>
      <StatusBar />
      <Label x={0} y={44} width={240} text_align="center" font="huge" color="muted">
        {format_number(display_hour.value, 2)}
      </Label>
      <Label x={0} y={140} width={240} text_align="center" font="huge">
        {format_number(minute_of(now.value), 2)}
      </Label>
    </Screen>
  );
}

export default define_app({
  id: "dual-clock",
  name: "Dual clock",
  icon: icons.clock,
  kind: "face",
  screens: { main: DualClockFace },
});
