// Lite rings watch face, ported from PinetimeLite's WatchFaceMinimal.
// Three rings fill clockwise from twelve: green for the hour, blue for the minute and red for the second.
// The second ring is wider than the screen, so only its middle shows, as in PinetimeLite.

import { computed } from "@preact/signals";
import {
  define_app,
  format_number,
  format_time,
  hour_of,
  minute_of,
  now,
  second_of,
  settings,
  unread_notifications,
  type Color,
} from "neotime";
import { icons } from "neotime/icons";
import { Arc, Icon, Label, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const HOUR_COLOR: Color = "#ffc300";
const MINUTE_COLOR: Color = "#00a3ff";
const DATE_COLOR: Color = "#1fff00";
const ICON_COLOR: Color = "#666666";

// ============================================================================
// Derived values
// ============================================================================

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

const hour_text = computed(() => {
  const hour = hour_of(now.value);
  if (uses_24_hour_clock.value) return format_number(hour, 2);
  return `${hour % 12 === 0 ? 12 : hour % 12}`;
});

/** A 12 hour clock writes the month before the day, as PinetimeLite does. */
const date_text = computed(() => format_time(now.value, uses_24_hour_clock.value ? "ddd, DD MMMM" : "ddd, MMMM DD"));

// ============================================================================
// Screen
// ============================================================================

/** A ring that fills clockwise from twelve as its value grows. */
function Ring(props: { value: number; maximum: number; diameter: number; color: Color }) {
  const offset = (240 - props.diameter) / 2;
  return (
    <Arc
      x={offset}
      y={offset}
      diameter={props.diameter}
      value={props.value}
      maximum={props.maximum}
      start_angle={0}
      end_angle={360}
      thickness={10}
      color={props.color}
      track_color="background"
    />
  );
}

function LiteRingsFace() {
  return (
    <Screen>
      <Ring value={second_of(now.value)} maximum={59} diameter={260} color="#ff0000" />
      <Ring value={minute_of(now.value)} maximum={59} diameter={232} color="#0000ff" />
      <Ring value={hour_of(now.value)} maximum={23} diameter={210} color="#00ff00" />

      <Label x={0} y={30} width={240} text_align="center" font="huge" color={HOUR_COLOR}>
        {hour_text.value}
      </Label>
      {!uses_24_hour_clock.value && (
        <Label x={176} y={56} font="small" color="#202020">
          {hour_of(now.value) >= 12 ? "PM" : "AM"}
        </Label>
      )}
      <Label x={0} y={109} width={240} text_align="center" color={DATE_COLOR}>
        {date_text.value}
      </Label>
      <Label x={0} y={134} width={240} text_align="center" font="huge" color={MINUTE_COLOR}>
        {format_number(minute_of(now.value), 2)}
      </Label>

      <Icon x={4} y={216} icon={icons.battery} size={20} color={ICON_COLOR} />
      {unread_notifications.value > 0 && <Icon x={216} y={216} icon={icons.notification} size={20} color={ICON_COLOR} />}
    </Screen>
  );
}

export default define_app({
  id: "lite-rings",
  name: "Lite Rings",
  icon: icons.clock,
  kind: "face",
  screens: { main: LiteRingsFace },
});
