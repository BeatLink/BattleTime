// Lite analog watch face, ported from PinetimeLite's WatchFaceAnalog and its two dials.
// The "classic" dial has white ticks, blue marks at 12, 3, 6 and 9, a "PineTime" label and the weekday and day in orange.
// The "rays" dial has grey rays, yellow hands and the step count; a long press switches between them.
// A double tap hides the second hand, so the hands move once a minute.

import { computed } from "@preact/signals";
import {
  define_app,
  format_time,
  now,
  persisted,
  start_of_minute,
  steps_today,
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

type DialStyle = "classic" | "rays";

const BLUE: Color = "#3399ff";
const ORANGE: Color = "#f0a500";
const DARK_GRAY: Color = "#2f2f2f";
const RAY_GRAY: Color = "#262626";
const YELLOW: Color = "#ffff00";
const RED: Color = "#ff0000";


/** PinetimeLite's hand lengths in pixels from the centre. */
const HOUR_LENGTH = 70;
const MINUTE_LENGTH = 90;
const SECOND_LENGTH = 110;

/** Which of PinetimeLite's two dials is drawn. */
const dial_style = persisted<DialStyle>("dial_style", "classic");

/** Each hour's ray, from 70 to 170 pixels out from the centre, written out because the compiler keeps only literal points. */
const RAYS = [
  [{ x: 120, y: 50 }, { x: 120, y: -50 }],
  [{ x: 155, y: 59 }, { x: 205, y: -27 }],
  [{ x: 181, y: 85 }, { x: 267, y: 35 }],
  [{ x: 190, y: 120 }, { x: 290, y: 120 }],
  [{ x: 181, y: 155 }, { x: 267, y: 205 }],
  [{ x: 155, y: 181 }, { x: 205, y: 267 }],
  [{ x: 120, y: 190 }, { x: 120, y: 290 }],
  [{ x: 85, y: 181 }, { x: 35, y: 267 }],
  [{ x: 59, y: 155 }, { x: -27, y: 205 }],
  [{ x: 50, y: 120 }, { x: -50, y: 120 }],
  [{ x: 59, y: 85 }, { x: -27, y: 35 }],
  [{ x: 85, y: 59 }, { x: 35, y: -27 }],
];

/** Whether the second hand is drawn. */
const shows_seconds = persisted("shows_seconds", true);

// ============================================================================
// Derived values
// ============================================================================

/** The short weekday in capitals over the day of the month. */
const date_text = computed(() => to_upper(format_time(now.value, "ddd\nDD")));

const date_color = computed(() => (dial_style.value === "classic" ? ORANGE : DARK_GRAY));

// ============================================================================
// Handlers
// ============================================================================

function switch_dial() {
  dial_style.value = dial_style.value === "classic" ? "rays" : "classic";
}

function toggle_seconds() {
  shows_seconds.value = !shows_seconds.value;
}

// ============================================================================
// Screen
// ============================================================================

function ClassicDial() {
  return (
    <>
      <Scale ticks={60} major_every={5} color="#808080" major_color="#ffffff" />
      <Line points={[{ x: 2, y: 120 }, { x: 20, y: 120 }]} thickness={4} color={BLUE} rounded={false} />
      <Line points={[{ x: 220, y: 120 }, { x: 238, y: 120 }]} thickness={4} color={BLUE} rounded={false} />
      <Line points={[{ x: 120, y: 220 }, { x: 120, y: 238 }]} thickness={4} color={BLUE} rounded={false} />
      <Label x={0} y={14} width={240} text_align="center" color={BLUE} font="large">
        12
      </Label>
      <Label x={0} y={196} width={240} text_align="center" color={BLUE}>
        PineTime
      </Label>
    </>
  );
}

/** Twelve grey rays from near the middle to the edge, one for each hour. */
function RaysDial() {
  return (
    <>
      {RAYS.map((ray) => (
        <Line points={ray} thickness={3} color={RAY_GRAY} />
      ))}
      <Label x={0} y={196} width={240} text_align="center" color={DARK_GRAY}>
        {`STEPS\n${steps_today.value}`}
      </Label>
    </>
  );
}

function LiteAnalogFace() {
  const is_classic = dial_style.value === "classic";
  return (
    <Screen onLongPress={switch_dial} onDoubleClick={toggle_seconds}>
      {is_classic ? <ClassicDial /> : <RaysDial />}

      {unread_notifications.value > 0 && <Icon x={0} y={0} icon={icons.notification} size={20} color="#00ff00" />}
      <Row x={196} y={218} width={40} height={20} justify="end">
        <BatteryIcon color={is_classic ? "text" : DARK_GRAY} single_color show_bolt={false} />
      </Row>

      <Label x={150} y={97} width={50} text_align="center" color={date_color.value}>
        {date_text.value}
      </Label>

      <AnalogClock
        time={shows_seconds.value ? now.value : start_of_minute(now.value)}
        show_seconds={shows_seconds.value}
        hour_length={HOUR_LENGTH}
        minute_length={MINUTE_LENGTH}
        second_length={SECOND_LENGTH}
        hand_style="rounded"
        hour_color={is_classic ? "#ffffff" : YELLOW}
        minute_color={is_classic ? "#ffffff" : YELLOW}
        second_color={RED}
      />
    </Screen>
  );
}

export default define_app({
  id: "lite-analog",
  name: "Lite Analog",
  icon: icons.clock,
  kind: "face",
  screens: { main: LiteAnalogFace },
});
