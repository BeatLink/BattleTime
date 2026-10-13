// Stopwatch with laps, merged from InfiniTime's StopWatch and wasp-os's stopwatch.
// It stores when it started rather than ticking, so it keeps running while the app is closed.

import { computed } from "@preact/signals";
import {
  define_app,
  format_duration,
  format_number,
  keep_screen_on,
  now,
  persisted,
  second_of,
  unshift_pop,
  HOUR,
  type Duration,
  type Instant,
} from "neotime";
import { icons } from "neotime/icons";
import { Button, Column, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const LAPS_SHOWN = 4;

/** When the current run started, shifted back by any time already counted, or null while stopped. */
const started_at = persisted<Instant | null>("started_at", null);

/** The time counted when the stopwatch was paused. */
const paused_after = persisted<Duration>("paused_after", 0);

/** The latest laps, newest first, as times since the start; 0 marks an empty place. */
const laps = persisted<Duration[]>("laps", [0, 0, 0, 0]);

/** How many laps have been taken, so the shown ones keep their numbers. */
const lap_count = persisted("lap_count", 0);

// ============================================================================
// Derived values
// ============================================================================

const is_running = computed(() => started_at.value !== null);

const is_cleared = computed(() => !is_running.value && paused_after.value === 0);

const elapsed = computed(() =>
  started_at.value !== null ? now.value - started_at.value : paused_after.value,
);

const shows_hours = computed(() => elapsed.value >= HOUR);

/** While paused the time blinks once a second, as on InfiniTime. */
const is_dimmed = computed(
  () => is_cleared.value || (!is_running.value && second_of(now.value) % 2 === 1),
);

// ============================================================================
// Handlers
// ============================================================================

function start_or_pause() {
  if (started_at.value !== null) {
    paused_after.value = now.value - started_at.value;
    started_at.value = null;
    keep_screen_on(false);
  } else {
    started_at.value = now.value - paused_after.value;
    keep_screen_on(true);
  }
}

function lap_or_reset() {
  if (started_at.value !== null) {
    laps.value = unshift_pop(laps.value, now.value - started_at.value);
    lap_count.value = lap_count.value + 1;
  } else {
    paused_after.value = 0;
    laps.value = [0, 0, 0, 0];
    lap_count.value = 0;
  }
}

// ============================================================================
// Screen
// ============================================================================

function LapRow(props: { place: number }) {
  const lap_time = laps.value[props.place];
  const previous = props.place + 1 < LAPS_SHOWN ? laps.value[props.place + 1] : 0;
  return (
    <>
      {lap_time > 0 && (
        <Row justify="space_between" padding={2}>
          <Label font="small" color="muted">{`#${lap_count.value - props.place}`}</Label>
          <Label font="small" color="muted">{format_duration(lap_time - previous, "mm:ss.cc")}</Label>
          <Label font="small">{format_duration(lap_time, "mm:ss.cc")}</Label>
        </Row>
      )}
    </>
  );
}

function Stopwatch() {
  return (
    <Screen padding={8} gap={6}
      onButton={(event) => {
        if (event.kind === "press") start_or_pause();
      }}
    >
      <Column align_items="center">
        <Label font={shows_hours.value ? "large" : "seven_segment"} color={is_dimmed.value ? "muted" : "text"}>
          {format_duration(elapsed.value, shows_hours.value ? "h:mm:ss" : "mm:ss")}
        </Label>
        <Label color={is_dimmed.value ? "muted" : "text"}>{format_number(Math.floor(elapsed.value / 10) % 100, 2)}</Label>
      </Column>
      <Column grow={1} justify="end" padding={4}>
        <LapRow place={0} />
        <LapRow place={1} />
        <LapRow place={2} />
        {!shows_hours.value && <LapRow place={3} />}
      </Column>
      <Row gap={6} height={64}>
        <Button
          grow={1}
          height="100%"
          disabled={is_cleared.value}
          background={is_running.value || is_cleared.value ? "surface" : "danger"}
          onClick={lap_or_reset}
        >
          <Icon icon={is_running.value || is_cleared.value ? icons.flag : icons.stop} />
        </Button>
        <Button
          grow={1}
          height="100%"
          background={is_running.value ? "surface" : "accent"}
          onClick={start_or_pause}
        >
          <Icon icon={is_running.value ? icons.pause : icons.play} />
        </Button>
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "stopwatch",
  name: "Stopwatch",
  group: "time",
  icon: icons.stopwatch,
  screens: { main: Stopwatch },
  onOpen: () => keep_screen_on(is_running.value),
});
