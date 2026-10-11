// Lite analog watch face, ported from PinetimeLite's WatchFaceAnalog and its two dials.
// The "classic" dial has white ticks, blue marks at 12, 3, 6 and 9, a "PineTime" label and the weekday and day in orange.
// The "rays" dial has grey rays, yellow hands and the step count; a long press switches between them.

import { computed } from "@preact/signals";
import {
  battery_percent,
  day_of,
  define_app,
  format_number,
  format_time,
  is_charging,
  now,
  persisted,
  steps_today,
  unread_notifications,
  type Color,
} from "neotime";
import { point_on_circle } from "neotime/lib";
import { icons } from "neotime/icons";
import { AnalogClock, Column, Icon, Label, Line, Row, Scale, Screen } from "neotime/widgets";

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

const CENTER = 120;

/** Which of PinetimeLite's two dials is drawn. */
const dial_style = persisted<DialStyle>("dial_style", "classic");

// ============================================================================
// Derived values
// ============================================================================

const date_text = computed(() => `${format_time(now.value, "ddd").toUpperCase()}\n${format_number(day_of(now.value), 2)}`);

const date_color = computed(() => (dial_style.value === "classic" ? ORANGE : DARK_GRAY));

// ============================================================================
// Handlers
// ============================================================================

function switch_dial() {
  dial_style.value = dial_style.value === "classic" ? "rays" : "classic";
}

// ============================================================================
// Screen
// ============================================================================

function BatteryLevel(props: { color: Color }) {
  return (
    <Row width={26} height={14} border_width={2} border_color={props.color} radius={2} padding={1}>
      <Column width={Math.max(1, Math.floor((battery_percent.value * 20) / 100))} height={8} background={props.color} />
    </Row>
  );
}

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
      {Array.from({ length: 12 }, (_, hour) => (
        <Line
          points={[point_on_circle(CENTER, CENTER, 70, hour * 30), point_on_circle(CENTER, CENTER, 170, hour * 30)]}
          thickness={3}
          color={RAY_GRAY}
        />
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
    <Screen onLongPress={switch_dial}>
      {is_classic ? <ClassicDial /> : <RaysDial />}

      {unread_notifications.value > 0 && <Icon x={0} y={0} icon={icons.notification} size={20} color="#00ff00" />}
      <Row x={204} y={218} width={36} height={20} justify="end">
        {is_charging.value ? (
          <Icon icon={icons.charging} size={20} color={is_classic ? "text" : DARK_GRAY} />
        ) : (
          <BatteryLevel color={is_classic ? "text" : DARK_GRAY} />
        )}
      </Row>

      <Label x={150} y={97} width={50} text_align="center" color={date_color.value}>
        {date_text.value}
      </Label>

      <AnalogClock
        time={now.value}
        show_seconds
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
