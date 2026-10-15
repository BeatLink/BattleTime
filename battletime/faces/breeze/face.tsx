// Breeze watch face, ported from ZephyrLabs' Breeze (ZephyrLabs/Watchfaces), after KDE's Breeze analog clock.
// A pale round dial on black with fine minute ticks, darker hour ticks, a blue "12" and blue marks at 3, 6 and 9.
// A charcoal hour hand, a grey minute hand and an orange second hand, with the heart rate and steps below the dial.

import { computed, define_app, heart_rate, is_heart_rate_running, now, type Color } from "battletime";
import { StatusIcons, StepsBadge } from "battletime/components";
import { icons } from "battletime/icons";
import { AnalogClock, Badge, Column, Label, Line, Scale, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

const BLACK: Color = "#000000";
const DIAL: Color = "#eff0f1";
const DIAL_EDGE: Color = "#c4cacc";
const BLUE: Color = "#3daee9";
const MINUTE_TICK: Color = "#a0a4a8";
const HOUR_TICK: Color = "#31363b";
const CHARCOAL: Color = "#232629";
const GREY: Color = "#717171";
const ORANGE: Color = "#f87c28";
const WHITE: Color = "#ffffff";

/** Breeze's hand lengths in pixels from the centre. */
const HOUR_LENGTH = 47;
const MINUTE_LENGTH = 52;
const SECOND_LENGTH = 57;

/** Every fifth minute tick is left to the hour ring. */
const HOUR_PLACES = [0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55];

/** The hour ticks under the "12" and the blue marks at 3, 6 and 9. */
const MARKED_HOURS = [0, 3, 6, 9];

// ============================================================================
// Derived values
// ============================================================================

/** The heart rate in beats per minute, or dashes while the sensor is off, as InfiniTime shows it. */
const heart_rate_text = computed(() => (is_heart_rate_running.value ? `${heart_rate.value} BPM` : "--- BPM"));

// ============================================================================
// Screen
// ============================================================================

function Dial() {
  return (
    <>
      <Column
        x={21}
        y={21}
        width={198}
        height={198}
        radius={99}
        background={DIAL}
        border_width={1}
        border_color={DIAL_EDGE}
      />
      <Scale x={31} y={31} diameter={178} ticks={60} skip={HOUR_PLACES} color={MINUTE_TICK} />
      <Scale x={31} y={31} diameter={178} ticks={12} skip={MARKED_HOURS} color={HOUR_TICK} />
      <Line points={[{ x: 28, y: 120 }, { x: 48, y: 120 }]} thickness={3} color={BLUE} rounded={false} />
      <Line points={[{ x: 192, y: 120 }, { x: 212, y: 120 }]} thickness={3} color={BLUE} rounded={false} />
      <Line points={[{ x: 120, y: 192 }, { x: 120, y: 212 }]} thickness={3} color={BLUE} rounded={false} />
      <Label x={0} y={23} width={240} text_align="center" font="large" color={BLUE}>
        12
      </Label>
    </>
  );
}

function BreezeFace() {
  return (
    <Screen background={BLACK}>
      <Dial />

      <StatusIcons x={140} y={0} width={96} height={22} align="end" gap={5} icon_size={18} color={WHITE} />

      <Badge x={4} y={214} height={24} icon={icons.heart} icon_size={20} gap={5} color={WHITE}>
        {heart_rate_text.value}
      </Badge>
      <StepsBadge x={140} y={214} width={96} height={24} align="end" color={WHITE} />

      <AnalogClock
        time={now.value}
        show_seconds
        hour_length={HOUR_LENGTH}
        minute_length={MINUTE_LENGTH}
        hand_style="rounded"
        hour_color={CHARCOAL}
        minute_color={GREY}
        second_length={SECOND_LENGTH}
        second_color={ORANGE}
      />
    </Screen>
  );
}

export default define_app({
  id: "breeze",
  name: "Breeze",
  icon: icons.clock,
  kind: "face",
  screens: { main: BreezeFace },
});
