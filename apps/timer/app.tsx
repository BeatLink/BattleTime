// Countdown timer, merged from wasp-os's timer and InfiniTime's Timer.
// From wasp-os: minutes up to 99, and a tap anywhere silences the ring.
// From InfiniTime: pause and resume, a running count of time since the end, and the buzzing stops itself.
// It stores when it ends rather than ticking, so it keeps running while the app is closed.

import { computed } from "@preact/signals";
import {
  at,
  back,
  define_app,
  format_duration,
  now,
  persisted,
  stop_vibrating,
  vibrate_pattern,
  wake_screen,
  MINUTE,
  SECOND,
  type Duration,
  type Instant,
  type TimeEvent,
} from "neotime";
import { DurationPicker } from "neotime/components";
import { icons } from "neotime/icons";
import { Arc, Button, Column, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** How long the ring buzzes before it falls silent on its own. */
const RING_LIMIT: Duration = MINUTE;

/** The length chosen on the rollers. */
export const chosen_duration = persisted<Duration>("chosen_duration", 10 * MINUTE);

/** The length of the run in progress, for the progress ring. */
const run_duration = persisted<Duration>("run_duration", 0);

/** When the running timer ends, or null while it is not running. */
const ends_at = persisted<Instant | null>("ends_at", null);

/** The time left when the timer was paused, or 0 if it is not paused. */
const paused_with = persisted<Duration>("paused_with", 0);

/** When the timer ended and began ringing, or null while it is not ringing. */
export const rang_at = persisted<Instant | null>("rang_at", null);

// ============================================================================
// Derived values
// ============================================================================

const is_running = computed(() => ends_at.value !== null);

const is_paused = computed(() => paused_with.value > 0);

const is_ringing = computed(() => rang_at.value !== null);

const is_choosing = computed(() => !is_running.value && !is_paused.value && !is_ringing.value);

/** The time left, rounded up to the second so the display reads 00:00 only as it rings. */
export const remaining = computed(() => {
  const left = ends_at.value !== null ? Math.max(0, ends_at.value - now.value) : paused_with.value;
  return Math.floor((left + SECOND - 1) / SECOND) * SECOND;
});

const overtime = computed(() => (rang_at.value !== null ? now.value - rang_at.value : 0));

const silence_at = computed(() => (rang_at.value !== null ? rang_at.value + RING_LIMIT : null));

// ============================================================================
// Handlers
// ============================================================================

export function start() {
  if (chosen_duration.value <= 0) return;
  run_duration.value = chosen_duration.value;
  ends_at.value = now.value + chosen_duration.value;
}

export function pause_or_resume() {
  if (ends_at.value !== null) {
    paused_with.value = Math.max(SECOND, ends_at.value - now.value);
    ends_at.value = null;
  } else {
    ends_at.value = now.value + paused_with.value;
    paused_with.value = 0;
  }
}

function reset() {
  ends_at.value = null;
  paused_with.value = 0;
}

function ring(event: TimeEvent) {
  rang_at.value = ends_at.value !== null ? ends_at.value : event.time;
  ends_at.value = null;
  paused_with.value = 0;
  wake_screen();
  vibrate_pattern("alarm");
}

export function dismiss() {
  rang_at.value = null;
  stop_vibrating();
}

function silence() {
  stop_vibrating();
}

// ============================================================================
// Screen
// ============================================================================

function Timer() {
  return (
    <Screen padding={8} gap={6}
      onButton={() => {
        if (is_ringing.value) dismiss();
        else back();
      }}
      onClick={() => {
        if (is_ringing.value) dismiss();
      }}
    >
      {is_choosing.value && (
        <Column grow={1} justify="center" padding={4}>
          <DurationPicker value={chosen_duration} />
        </Column>
      )}
      {is_choosing.value && (
        <Row height={60} padding={4}>
          <Button grow={1} height="100%" background="success" disabled={chosen_duration.value <= 0} onClick={start}>
            <Icon icon={icons.play} />
          </Button>
        </Row>
      )}

      {(is_running.value || is_paused.value) && (
        <Column grow={1} align_items="center" justify="center">
          <Arc
            diameter={164}
            value={Math.floor(remaining.value / SECOND)}
            maximum={Math.max(1, Math.floor(run_duration.value / SECOND))}
            start_angle={0}
            end_angle={0}
            color={is_paused.value ? "muted" : "accent"}
          >
            <Label font="large" color={is_paused.value ? "muted" : "text"}>
              {format_duration(remaining.value, remaining.value >= 100 * MINUTE ? "h:mm:ss" : "mm:ss")}
            </Label>
          </Arc>
        </Column>
      )}
      {(is_running.value || is_paused.value) && (
        <Row gap={6} height={60} padding={4}>
          {is_paused.value && (
            <Button grow={1} height="100%" background="danger" onClick={reset}>
              <Icon icon={icons.stop} />
            </Button>
          )}
          <Button grow={1} height="100%" background={is_paused.value ? "accent" : "surface"} onClick={pause_or_resume}>
            <Icon icon={is_paused.value ? icons.play : icons.pause} />
          </Button>
        </Row>
      )}

      {is_ringing.value && (
        <Column grow={1} align_items="center" justify="center" gap={6}>
          <Icon icon={icons.timer} size={56} color="warning" />
          <Label>Time's up</Label>
          <Label font="large" color="muted">{`+${format_duration(overtime.value, "mm:ss")}`}</Label>
        </Column>
      )}
      {is_ringing.value && (
        <Row height={60} padding={4}>
          <Button grow={1} height="100%" background="danger" onClick={dismiss}>
            Dismiss
          </Button>
        </Row>
      )}
    </Screen>
  );
}

export default define_app({
  id: "timer",
  name: "Timer",
  group: "time",
  icon: icons.timer,
  screens: { main: Timer },
  subscriptions: [at(ends_at, ring), at(silence_at, silence)],
  permissions: ["wake", "background"],
});
