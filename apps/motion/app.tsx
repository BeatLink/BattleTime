// Accelerometer check, ported from InfiniTime's Motion.
// It charts the last ten readings of each axis, shows the latest in thousandths of g, and the step count.

import { signal } from "@preact/signals";
import { acceleration, define_app, steps_today, type AccelerationEvent } from "neotime";
import { icons } from "neotime/icons";
import { Chart, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const LOWEST_READING = -1100;
const HIGHEST_READING = 1100;
const X_COLOR = "#ff0000";
const Y_COLOR = "#00b000";
const Z_COLOR = "#ffff00";

/** The last ten readings of each axis, oldest first. */
const x_readings = signal<number[]>([0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
const y_readings = signal<number[]>([0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
const z_readings = signal<number[]>([0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

// ============================================================================
// Handlers
// ============================================================================

/** The readings moved one place older, with the new one at the end. */
function shifted(readings: number[], newest: number): number[] {
  return readings.map((_reading, place) => (place + 1 < readings.length ? readings[place + 1] : newest));
}

function record(event: AccelerationEvent) {
  x_readings.value = shifted(x_readings.value, event.x);
  y_readings.value = shifted(y_readings.value, event.y);
  z_readings.value = shifted(z_readings.value, event.z);
}

// ============================================================================
// Screen
// ============================================================================

function Motion() {
  return (
    <Screen>
      <Chart
        x={0}
        y={0}
        width={240}
        height={240}
        minimum={LOWEST_READING}
        maximum={HIGHEST_READING}
        series={[
          { values: x_readings.value, color: X_COLOR },
          { values: y_readings.value, color: Y_COLOR },
          { values: z_readings.value, color: Z_COLOR },
        ]}
      />
      <Row x={0} y={10} width={240} gap={6} justify="center">
        <Label font="small">X</Label>
        <Label font="small" color={X_COLOR}>{`${x_readings.value[9]}`}</Label>
        <Label font="small">Y</Label>
        <Label font="small" color={Y_COLOR}>{`${y_readings.value[9]}`}</Label>
        <Label font="small">Z</Label>
        <Label font="small" color={Z_COLOR}>{`${z_readings.value[9]}`}</Label>
        <Label font="small">mg</Label>
      </Row>
      <Label x={4} y={216} font="small">{`Steps ${steps_today.value}`}</Label>
    </Screen>
  );
}

export default define_app({
  id: "motion",
  name: "Motion",
  icon: icons.level,
  screens: { main: Motion },
  subscriptions: [acceleration({ milliseconds: 100 }, record, { only_while_open: true })],
  permissions: ["motion"],
});
