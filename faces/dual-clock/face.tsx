// Dual clock watch face, ported from wasp-os's dual_clock.
// The hours sit above the minutes in big digits, hours dimmer than minutes, with wasp-os's status bar above.

import { computed } from "@preact/signals";
import {
  battery_percent,
  define_app,
  format_number,
  hour_of,
  is_phone_connected,
  minute_of,
  now,
  settings,
  unread_notifications,
  type Color,
} from "neotime";
import { BatteryIcon } from "neotime/components";
import { icons } from "neotime/icons";
import { Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** wasp-os draws the hours in its theme's mid shade, about this much of the bright one on black. */
const HOURS_OPACITY = 185;

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
      <BatteryIcon color={battery_color.value} />
    </Row>
  );
}

function DualClockFace() {
  return (
    <Screen>
      <StatusBar />
      <Label x={0} y={44} width={240} text_align="center" font="huge" opacity={HOURS_OPACITY}>
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
