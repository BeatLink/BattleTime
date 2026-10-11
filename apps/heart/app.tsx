// Heart rate monitor, merged from InfiniTime's HeartRate and wasp-os's heart app.
// It starts measuring on open as wasp-os does, and has InfiniTime's large reading, status line and
// Start/Stop button; wasp-os's live graph becomes a chart of the latest readings.

import { computed, signal } from "@preact/signals";
import {
  define_app,
  format_number,
  heart_rate_reading,
  keep_screen_on,
  start_heart_rate,
  stop_heart_rate,
  type HeartRateEvent,
} from "neotime";
import { icons } from "neotime/icons";
import { Button, Chart, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** Readings below this confidence are shown as still settling. */
const SETTLED_CONFIDENCE = 50;

const is_measuring = signal(false);

/** The latest reading this session, or 0 before the first. */
const beats_per_minute = signal(0);

/** How sure the sensor was of the latest reading, 0 when it sees no skin. */
const confidence = signal(-1);

/** The latest readings, oldest first; 0 marks an empty place. */
const readings = signal<number[]>([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

// ============================================================================
// Derived values
// ============================================================================

const has_reading = computed(
  () => is_measuring.value && confidence.value >= SETTLED_CONFIDENCE && beats_per_minute.value > 0,
);

const reading_text = computed(() => (has_reading.value ? format_number(beats_per_minute.value, 3) : "---"));

const status_text = computed(() => {
  if (!is_measuring.value) return "Stopped";
  if (confidence.value === 0) return "No touch detected";
  if (confidence.value < SETTLED_CONFIDENCE) return "Not enough data, please wait...";
  return "Measuring...";
});

const has_history = computed(() => readings.value[readings.value.length - 1] > 0);

// ============================================================================
// Handlers
// ============================================================================

function start() {
  is_measuring.value = true;
  confidence.value = -1;
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
  confidence.value = event.confidence;
  beats_per_minute.value = event.beats_per_minute;
  if (event.confidence < SETTLED_CONFIDENCE || event.beats_per_minute === 0) return;
  readings.value = readings.value.map((_reading, place) =>
    place + 1 < readings.value.length ? readings.value[place + 1] : event.beats_per_minute,
  );
}

// ============================================================================
// Screen
// ============================================================================

function HeartRate() {
  return (
    <Screen
      padding={6}
      align_items="center"
      onButton={(event) => {
        if (event.kind === "press") start_or_stop();
      }}
    >
      <Row gap={6} justify="center">
        <Icon icon={icons.heart} size={20} color={is_measuring.value ? "danger" : "muted"} />
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
          <Chart width={228} height={48} values={readings.value} minimum={40} maximum={180} color="danger" />
        )}
      </Row>
      <Button width={228} height={50} background={is_measuring.value ? "surface" : "accent"} onClick={start_or_stop}>
        {is_measuring.value ? "Stop" : "Start"}
      </Button>
    </Screen>
  );
}

export default define_app({
  id: "heart",
  name: "Heart",
  icon: icons.heart,
  screens: { main: HeartRate },
  subscriptions: [heart_rate_reading({ seconds: 1 }, record)],
  permissions: ["heart_rate"],
  onOpen: start,
  onClose: stop,
});
