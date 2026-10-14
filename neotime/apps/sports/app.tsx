// Sports timer, ported from wasp-os's sports app: a stopwatch that also counts the steps taken while
// it runs. The Start button starts and stops it, and a tap clears it once stopped, keeping the
// session among the last few for the companion to sync.

import {
  computed,
  define_app,
  format_duration,
  keep_screen_on,
  now,
  on,
  persisted,
  range,
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

/** When the first run since the last clear started, or null before one has. */
const session_started_at = persisted<Instant | null>("session_started_at", null);

const SESSIONS_KEPT = 5;

/** One finished session: when it started, the time it counted and its steps. */
interface Session {
  started_at: Instant;
  duration: Duration;
  steps: number;
}

const session_count = persisted("session_count", 0);

/** The last finished sessions, newest first; only the first `session_count` are in use. */
const sessions = persisted<Session[]>("sessions", [
  { started_at: 0, duration: 0, steps: 0 },
  { started_at: 0, duration: 0, steps: 0 },
  { started_at: 0, duration: 0, steps: 0 },
  { started_at: 0, duration: 0, steps: 0 },
  { started_at: 0, duration: 0, steps: 0 },
]);

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
    if (session_started_at.value === null) {
      session_started_at.value = now.value;
    }
    started_at.value = now.value - stopped_after.value;
    steps_at_start.value = steps_today.value;
    keep_screen_on(true);
  }
}

/** Keeps the session being cleared as the newest, moving the others down a place and dropping the oldest. */
function keep_session() {
  for (const each of range(SESSIONS_KEPT - 1)) {
    const place = SESSIONS_KEPT - 1 - each;
    sessions.value[place].started_at = sessions.value[place - 1].started_at;
    sessions.value[place].duration = sessions.value[place - 1].duration;
    sessions.value[place].steps = sessions.value[place - 1].steps;
  }
  sessions.value[0].started_at = session_started_at.value ?? now.value - stopped_after.value;
  sessions.value[0].duration = stopped_after.value;
  sessions.value[0].steps = steps_before.value;
  session_count.value = Math.min(session_count.value + 1, SESSIONS_KEPT);
}

function clear() {
  if (started_at.value !== null) return;
  if (stopped_after.value > 0) {
    keep_session();
  }
  stopped_after.value = 0;
  steps_before.value = 0;
  session_started_at.value = null;
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

on("open", () => keep_screen_on(is_running.value));

export default define_app({
  id: "sports",
  name: "Sports",
  group: "time",
  icon: icons.stopwatch,
  screens: { main: Sports },
});
