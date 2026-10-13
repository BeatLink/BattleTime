// Sports timer, ported from wasp-os's sports app: a stopwatch that also counts the steps taken while
// it runs. The button starts and stops it, and a tap clears it once stopped.

import { computed } from "@preact/signals";
import {
  define_app,
  format_duration,
  keep_screen_on,
  now,
  persisted,
  steps_today,
  HOUR,
  type Duration,
  type Instant,
} from "neotime";
import { steps_since } from "neotime/lib";
import { Card } from "neotime/components";
import { icons } from "neotime/icons";
import { Button, Column, Icon, Label, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** When the current run started, shifted back by any time already counted, or null while stopped. */
const started_at = persisted<Instant | null>("started_at", null);

/** The time counted when the timer was stopped. */
const stopped_after = persisted<Duration>("stopped_after", 0);

/** The day's step count when the current run started. */
const steps_at_start = persisted("steps_at_start", 0);

/** Steps counted by earlier runs since the last clear. */
const steps_before = persisted("steps_before", 0);

// ============================================================================
// Derived values
// ============================================================================

const is_running = computed(() => started_at.value !== null);

const elapsed = computed(() => (started_at.value !== null ? now.value - started_at.value : stopped_after.value));

/** A run that passes midnight keeps counting, because the day's count starts again from zero. */
const steps_this_run = computed(() => steps_since(steps_at_start.value));

const session_steps = computed(() => steps_before.value + (is_running.value ? steps_this_run.value : 0));

// ============================================================================
// Handlers
// ============================================================================

function start_or_stop() {
  if (started_at.value !== null) {
    stopped_after.value = now.value - started_at.value;
    steps_before.value = steps_before.value + steps_this_run.value;
    started_at.value = null;
    keep_screen_on(false);
  } else {
    started_at.value = now.value - stopped_after.value;
    steps_at_start.value = steps_today.value;
    keep_screen_on(true);
  }
}

function clear() {
  if (started_at.value !== null) return;
  stopped_after.value = 0;
  steps_before.value = 0;
}

// ============================================================================
// Screen
// ============================================================================

function Sports() {
  return (
    <Screen
      padding={8}
      gap={6}
      onClick={clear}
      onButton={(event) => {
        if (event.kind === "press") start_or_stop();
      }}
    >
      <Card grow={1} padding={6}>
        <Column grow={1} align_items="center" justify="center">
          <Label font="large">{format_duration(elapsed.value, elapsed.value >= HOUR ? "h:mm:ss" : "mm:ss.cc")}</Label>
        </Column>
      </Card>
      <Card grow={1} padding={6}>
        <Column grow={1} align_items="center" justify="center">
          <Label font="large">{`${session_steps.value}`}</Label>
          <Label font="small" color="muted">steps</Label>
        </Column>
      </Card>
      <Button height={44} background={is_running.value ? "surface" : "accent"} onClick={start_or_stop}>
        <Icon icon={is_running.value ? icons.pause : icons.play} />
      </Button>
    </Screen>
  );
}

export default define_app({
  id: "sports",
  name: "Sports",
  group: "time",
  icon: icons.stopwatch,
  screens: { main: Sports },
  onOpen: () => keep_screen_on(is_running.value),
});
