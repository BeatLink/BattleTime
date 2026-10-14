// Analog watch face, merged from InfiniTime's WatchFaceAnalog and wasp-os's chrono.
// InfiniTime gives the dial: minute and hour ticks, aqua marks at 3, 6 and 9, the "12", the date and status icons.
// wasp-os draws no second hand to save power, so a long press switches the second hand off and on.

import {
  computed,
  define_app,
  format_time,
  is_charging,
  is_phone_connected,
  now,
  persisted,
  start_of_minute,
  to_upper,
  unread_notifications,
  type Color,
} from "neotime";
import { BatteryIcon } from "neotime/components";
import { icons } from "neotime/icons";
import { AnalogClock, Icon, Label, Line, Row, Scale, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const AQUA: Color = "#00ffff";
const ORANGE: Color = "#ffb000";
const GRAY: Color = "#808080";

/** InfiniTime's hand lengths in pixels from the centre. */
const HOUR_LENGTH = 70;
const MINUTE_LENGTH = 90;
const SECOND_LENGTH = 110;

/** The gap around the "12" and the ticks under the aqua marks at 3, 6 and 9, counted in minutes. */
const SKIPPED_TICKS = [56, 57, 58, 59, 0, 1, 2, 3, 4, 15, 30, 45];

/** Whether the second hand is drawn; InfiniTime shows it, wasp-os does not. */
const shows_seconds = persisted("shows_seconds", true);

// ============================================================================
// Derived values
// ============================================================================

/** The short weekday in capitals over the day of the month, as InfiniTime shows it. */
const date_text = computed(() => to_upper(format_time(now.value, "ddd\nDD")));

// ============================================================================
// Handlers
// ============================================================================

function toggle_seconds() {
  shows_seconds.value = !shows_seconds.value;
}

// ============================================================================
// Screen
// ============================================================================

function AnalogFace() {
  return (
    <Screen onLongPress={toggle_seconds}>
      <Scale ticks={60} major_every={5} skip={SKIPPED_TICKS} color={GRAY} major_color="#ffffff" />
      <Line points={[{ x: 220, y: 120 }, { x: 238, y: 120 }]} thickness={4} color={AQUA} rounded={false} />
      <Line points={[{ x: 120, y: 220 }, { x: 120, y: 238 }]} thickness={4} color={AQUA} rounded={false} />
      <Line points={[{ x: 2, y: 120 }, { x: 20, y: 120 }]} thickness={4} color={AQUA} rounded={false} />
      <Label x={0} y={10} width={240} text_align="center" color={AQUA}>
        12
      </Label>

      {unread_notifications.value > 0 && <Icon x={0} y={0} icon={icons.notification} size={24} color="#00ff00" />}
      <Row x={130} y={0} width={110} height={24} justify="end" gap={8}>
        {is_phone_connected.value && <Icon icon={icons.bluetooth} size={20} />}
        {is_charging.value ? <Icon icon={icons.charging} size={20} /> : <BatteryIcon single_color />}
      </Row>

      <Label x={145} y={97} width={50} text_align="center" color={ORANGE}>
        {date_text.value}
      </Label>

      <AnalogClock
        time={shows_seconds.value ? now.value : start_of_minute(now.value)}
        show_seconds={shows_seconds.value}
        hour_length={HOUR_LENGTH}
        minute_length={MINUTE_LENGTH}
        second_length={SECOND_LENGTH}
        hand_style="rounded"
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
