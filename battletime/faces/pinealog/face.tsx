// Pinealog watch face, ported from ZephyrLabs' Pinealog (ZephyrLabs/Watchfaces), InfiniTime's first analog face.
// A cream and navy dial split on the diagonal, with orange numerals at 12, 3, 6 and 9 and the Pine64 pine cone.
// Orange hour and minute hands and a red second hand, the heart rate and steps below and the status icons above.

import { computed, define_app, heart_rate, is_heart_rate_running, now, type Color } from "battletime";
import { StatusIcons, StepsBadge } from "battletime/components";
import { icons } from "battletime/icons";
import { AnalogClock, Badge, Column, Label, Line, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

const CREAM: Color = "#e4d6bc";
const NAVY: Color = "#242e3c";
const ORANGE: Color = "#f47a2c";
const RED: Color = "#ed1c24";
const WHITE: Color = "#ffffff";

/** Pinealog's hand lengths in pixels from the centre. */
const HOUR_LENGTH = 50;
const MINUTE_LENGTH = 70;
const SECOND_LENGTH = 100;

/** The pine cone's crossing cream lines, joined outside the cone where they vanish into the cream. */
const CONE_LATTICE = [
  { x: 96, y: 80 },
  { x: 144, y: 104 },
  { x: 96, y: 104 },
  { x: 144, y: 80 },
  { x: 146, y: 77 },
  { x: 94, y: 51 },
  { x: 94, y: 65 },
  { x: 146, y: 91 },
  { x: 146, y: 65 },
  { x: 94, y: 91 },
  { x: 94, y: 77 },
  { x: 146, y: 51 },
];

// ============================================================================
// Derived values
// ============================================================================

/** The heart rate in beats per minute, or dashes while the sensor is off, as InfiniTime shows it. */
const heart_rate_text = computed(() => (is_heart_rate_running.value ? `${heart_rate.value} BPM` : "--- BPM"));

// ============================================================================
// Screen
// ============================================================================

/** The Pine64 pine cone: an orange body and cap cut into scales by crossing cream lines. */
function PineCone() {
  return (
    <>
      <Column x={100} y={63} width={40} height={38} background={ORANGE} radius={6} />
      <Line
        points={[
          { x: 114, y: 62 },
          { x: 120, y: 55 },
          { x: 126, y: 62 },
          { x: 117, y: 61 },
          { x: 120, y: 58 },
          { x: 123, y: 61 },
        ]}
        thickness={3}
        color={ORANGE}
      />
      <Line points={CONE_LATTICE} thickness={3} color={CREAM} />
    </>
  );
}

function PinealogFace() {
  return (
    <Screen background={CREAM}>
      {/* The navy half: one wide band whose top edge runs from the lower left to the upper right. */}
      <Line points={[{ x: -10, y: 317 }, { x: 350, y: 77 }]} thickness={180} color={NAVY} rounded={false} />

      <Label x={0} y={9} width={240} text_align="center" font="medium" color={ORANGE}>
        12
      </Label>
      <Label x={206} y={100} width={30} text_align="center" font="medium" color={ORANGE}>
        3
      </Label>
      <Label x={0} y={186} width={240} text_align="center" font="medium" color={ORANGE}>
        6
      </Label>
      <Label x={4} y={100} width={30} text_align="center" font="medium" color={ORANGE}>
        9
      </Label>
      <PineCone />

      <StatusIcons x={120} y={0} width={116} height={24} align="end" gap={5} icon_size={18} color={WHITE} />

      <Badge x={4} y={214} height={24} icon={icons.heart} icon_size={20} gap={5} color={WHITE}>
        {heart_rate_text.value}
      </Badge>
      <StepsBadge x={120} y={214} width={116} height={24} align="end" color={WHITE} />

      <AnalogClock
        time={now.value}
        show_seconds
        hour_length={HOUR_LENGTH}
        minute_length={MINUTE_LENGTH}
        hand_style="rounded"
        hour_color={ORANGE}
        minute_color={ORANGE}
        second_length={SECOND_LENGTH}
        second_color={RED}
      />
    </Screen>
  );
}

export default define_app({
  id: "pinealog",
  name: "Pinealog",
  icon: icons.clock,
  kind: "face",
  screens: { main: PinealogFace },
});
