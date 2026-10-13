// Heart rate monitor, merged from InfiniTime's HeartRate and wasp-os's heart app.
// It starts measuring on open as wasp-os does, and has InfiniTime's large reading, status line and
// Start/Stop button; wasp-os's live graph becomes a chart of the latest readings.

import { computed, signal } from "@preact/signals";
import {
  define_app,
  format_number,
  heart_rate_reading,
  heart_rate_status,
  is_heart_rate_running,
  keep_screen_on,
  push_shift,
  start_heart_rate,
  stop_heart_rate,
  type HeartRateEvent,
} from "neotime";
import { icons } from "neotime/icons";
import { Button, Chart, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** The sensor states in `heart_rate_status`. */
const SENSOR_NO_SKIN = 1;
const SENSOR_SETTLING = 2;
const SENSOR_MEASURING = 3;

/** Whether this app asked for readings, which the Start button turns on and off. */
const is_measuring = signal(false);

/** The latest reading this session, or 0 before the first. */
const beats_per_minute = signal(0);

/** The latest readings, oldest first; 0 marks an empty place. */
const readings = signal<number[]>([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

// ============================================================================
// Derived values
// ============================================================================

const has_reading = computed(
  () => is_measuring.value && heart_rate_status.value === SENSOR_MEASURING && beats_per_minute.value > 0,
);

const reading_text = computed(() => (has_reading.value ? format_number(beats_per_minute.value, 3) : "---"));

const status_text = computed(() => {
  if (!is_measuring.value) return "Stopped";
  if (heart_rate_status.value === SENSOR_NO_SKIN) return "No touch detected";
  if (heart_rate_status.value === SENSOR_SETTLING) return "Not enough data, please wait...";
  if (heart_rate_status.value === SENSOR_MEASURING) return "Measuring...";
  return "Starting...";
});

const has_history = computed(() => readings.value[readings.value.length - 1] > 0);

// ============================================================================
// Handlers
// ============================================================================

function start() {
  is_measuring.value = true;
  start_heart_rate();
  keep_screen_on(true);
}

function stop() {
  is_measuring.value = false;
  stop_heart_rate();
  keep_screen_on(false);
}

function start_or_stop() {
  if (is_measuring.value) stop();
  else start();
}

function record(event: HeartRateEvent) {
  if (!is_measuring.value) return;
  beats_per_minute.value = event.beats_per_minute;
  if (heart_rate_status.value !== SENSOR_MEASURING || event.beats_per_minute === 0) return;
  readings.value = push_shift(readings.value, event.beats_per_minute);
}

// ============================================================================
// Screen
// ============================================================================

function HeartRate() {
  return (
    <Screen
      padding={8} gap={6}
      align_items="center"
    >
      <Row gap={6} justify="center">
        <Icon icon={icons.heart} size={20} color={is_heart_rate_running.value ? "danger" : "muted"} />
        <Label font="small" color="muted">Heart rate BPM</Label>
      </Row>
      <Label font="huge" color={is_measuring.value ? "#00b000" : "#b0b0b0"}>
        {reading_text.value}
      </Label>
      <Label font="small" color="muted" text_align="center">
        {status_text.value}
      </Label>
      <Row grow={1} align_items="end" justify="center">
        {has_history.value && (
          <Chart width={224} height={48} values={readings.value} minimum={40} maximum={180} color="danger" />
        )}
      </Row>
      <Button width={224} height={50} background={is_measuring.value ? "surface" : "accent"} onClick={start_or_stop}>
        {is_measuring.value ? "Stop" : "Start"}
      </Button>
    </Screen>
  );
}

export default define_app({
  id: "heart",
  name: "Heart",
  group: "health",
  icon: icons.heart,
  screens: { main: HeartRate },
  subscriptions: [heart_rate_reading({ seconds: 1 }, record, { only_while_open: true })],
  permissions: ["heart_rate"],
  onOpen: start,
  onClose: stop,
});
