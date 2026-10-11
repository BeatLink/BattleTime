// Analog watch face, merged from InfiniTime's WatchFaceAnalog and wasp-os's chrono.
// InfiniTime gives the dial: minute and hour ticks, aqua marks at 3, 6 and 9, the "12", the date and status icons.
// wasp-os draws no second hand to save power, so a long press switches the second hand off and on.

import { computed } from "@preact/signals";
import {
  battery_percent,
  define_app,
  day_of,
  format_number,
  is_charging,
  is_phone_connected,
  now,
  persisted,
  unread_notifications,
  weekday_of,
  type Color,
} from "neotime";
import { icons } from "neotime/icons";
import { AnalogClock, Column, Icon, Label, Line, Row, Scale, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const AQUA: Color = "#00ffff";
const ORANGE: Color = "#ffb000";
const GRAY: Color = "#808080";

const WEEKDAY_NAMES = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];

/** Whether the second hand is drawn; InfiniTime shows it, wasp-os does not. */
const shows_seconds = persisted("shows_seconds", true);

// ============================================================================
// Derived values
// ============================================================================

/** The short weekday over the day of the month, as InfiniTime shows it. */
const date_text = computed(
  () => `${WEEKDAY_NAMES[weekday_of(now.value)]}\n${format_number(day_of(now.value), 2)}`,
);

// ============================================================================
// Handlers
// ============================================================================

function toggle_seconds() {
  shows_seconds.value = !shows_seconds.value;
}

// ============================================================================
// Screen
// ============================================================================

function BatteryLevel() {
  return (
    <Row width={26} height={14} border_width={2} border_color="text" radius={2} padding={1}>
      <Column width={Math.max(1, Math.floor((battery_percent.value * 20) / 100))} height={8} background="text" />
    </Row>
  );
}

function AnalogFace() {
  return (
    <Screen onLongPress={toggle_seconds}>
      <Scale ticks={60} major_every={5} color={GRAY} major_color="#ffffff" />
      <Line points={[{ x: 220, y: 120 }, { x: 238, y: 120 }]} thickness={4} color={AQUA} rounded={false} />
      <Line points={[{ x: 120, y: 220 }, { x: 120, y: 238 }]} thickness={4} color={AQUA} rounded={false} />
      <Line points={[{ x: 2, y: 120 }, { x: 20, y: 120 }]} thickness={4} color={AQUA} rounded={false} />
      <Label x={0} y={20} width={240} text_align="center" color={AQUA}>
        12
      </Label>

      {unread_notifications.value > 0 && <Icon x={0} y={0} icon={icons.notification} size={24} color="#00ff00" />}
      <Row x={150} y={0} width={90} height={24} justify="end" gap={8}>
        {is_phone_connected.value && <Icon icon={icons.bluetooth} size={20} />}
        {is_charging.value ? <Icon icon={icons.charging} size={20} /> : <BatteryLevel />}
      </Row>

      <Label x={145} y={97} width={50} text_align="center" color={ORANGE}>
        {date_text.value}
      </Label>

      <AnalogClock
        time={now.value}
        show_seconds={shows_seconds.value}
        hour_color="#ffffff"
        minute_color="#ffffff"
        second_color="#ff0000"
      />
    </Screen>
  );
}

export default define_app({
  id: "analog",
  name: "Analog",
  icon: icons.clock,
  kind: "face",
  screens: { main: AnalogFace },
});
