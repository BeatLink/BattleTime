// Pomodoro timer, ported from wasp-os's pomodoro app.
// Type a queue of lengths in minutes, such as 25 then 5, and it runs them in turn, buzzing between
// each, until stopped or after 99 rounds. Swipe left or right for a preset queue, and up or down to
// change how many seconds each buzz lasts.

import { computed } from "@preact/signals";
import {
  at,
  define_app,
  format_duration,
  format_number,
  now,
  persisted,
  range,
  stop_vibrating,
  vibrate,
  vibrate_pattern,
  wake_screen,
  MINUTE,
  SECOND,
  type Instant,
  type SwipeEvent,
  type TimeEvent,
} from "neotime";
import { Card } from "neotime/components";
import { icons } from "neotime/icons";
import { Button, ButtonMatrix, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const QUEUE_LENGTH_LIMIT = 6;

/** The largest length one entry may have, in minutes. */
const LONGEST_ENTRY_MINUTES = 999;

const ROUND_LIMIT = 99;
const LONGEST_BUZZ_SECONDS = 15;

/** The places of a queue's entries. */
const QUEUE_PLACES = range(QUEUE_LENGTH_LIMIT);

/** Preset queues one after another, each padded with zeros to six entries, and how many entries each has. */
const PRESETS = [
  25, 5, 0, 0, 0, 0,
  50, 10, 0, 0, 0, 0,
  25, 5, 25, 5, 25, 15,
];
const PRESET_LENGTHS = [2, 2, 6];
const PRESET_COUNT = 3;

const KEYPAD_ROWS = [
  ["1", "2", "3", "4", "5"],
  ["6", "7", "8", "9", "0"],
  ["Del", "Then", "Go"],
];
const KEY_DELETE = 10;
const KEY_THEN = 11;
const KEY_GO = 12;

/** The queue's lengths in minutes; places past `queue_length` hold 0. */
const queue_minutes = persisted<number[]>("queue_minutes", [25, 5, 0, 0, 0, 0]);
const queue_length = persisted("queue_length", 2);

/** Whether "Then" was the last key, so the next digit starts a new entry. */
const awaits_next_entry = persisted("awaits_next_entry", false);

const preset_place = persisted("preset_place", 0);

/** How many seconds each buzz lasts. */
const buzz_seconds = persisted("buzz_seconds", 10);

/** When the running entry ends, or null while none runs. */
export const ends_at = persisted<Instant | null>("ends_at", null);

/** When the last entry ended and the buzz began, or null while not buzzing. */
export const rang_at = persisted<Instant | null>("rang_at", null);

/** Which entry of the queue runs now. */
export const running_place = persisted("running_place", 0);

/** How many times the whole queue has run. */
export const rounds_done = persisted("rounds_done", 0);

// ============================================================================
// Derived values
// ============================================================================

const is_running = computed(() => ends_at.value !== null);

const is_ringing = computed(() => rang_at.value !== null);

const is_stopped = computed(() => !is_running.value && !is_ringing.value);

const can_start = computed(() => queue_length.value > 0 && !awaits_next_entry.value);

/** One entry of the queue as text, with a comma before all but the first, or nothing past the queue's end. */
function queue_entry_text(place: number): string {
  return place < queue_length.value ? `${place > 0 ? "," : ""}${format_number(queue_minutes.value[place])}` : "";
}

export const queue_text = computed(
  () =>
    `${queue_entry_text(0)}${queue_entry_text(1)}${queue_entry_text(2)}${queue_entry_text(3)}${queue_entry_text(4)}${queue_entry_text(5)}${awaits_next_entry.value ? "," : ""}`,
);

const remaining = computed(() =>
  ends_at.value !== null ? Math.floor((Math.max(0, ends_at.value - now.value) + SECOND - 1) / SECOND) * SECOND : 0,
);

const progress_text = computed(() => `Timer #${running_place.value + 1}/${queue_length.value}  (${rounds_done.value})`);

/** When the buzz ends and the next entry starts. */
const next_entry_at = computed(() => (rang_at.value !== null ? rang_at.value + buzz_seconds.value * SECOND : null));

// ============================================================================
// Handlers
// ============================================================================

function type_digit(digit: number) {
  const last = queue_length.value - 1;
  if (awaits_next_entry.value || queue_length.value === 0) {
    if (digit === 0 || queue_length.value >= QUEUE_LENGTH_LIMIT) return;
    queue_minutes.value[queue_length.value] = digit;
    queue_length.value = queue_length.value + 1;
    awaits_next_entry.value = false;
  } else if (queue_minutes.value[last] * 10 + digit <= LONGEST_ENTRY_MINUTES) {
    queue_minutes.value[last] = queue_minutes.value[last] * 10 + digit;
  }
}

function delete_key() {
  const last = queue_length.value - 1;
  if (awaits_next_entry.value) {
    awaits_next_entry.value = false;
  } else if (queue_length.value > 0) {
    const shortened = Math.floor(queue_minutes.value[last] / 10);
    queue_minutes.value[last] = shortened;
    if (shortened === 0) {
      queue_length.value = last;
      awaits_next_entry.value = last > 0;
    }
  }
}

function then_key() {
  if (queue_length.value > 0 && queue_length.value < QUEUE_LENGTH_LIMIT) {
    awaits_next_entry.value = true;
  }
}

export function press_key(event: { value: number }) {
  if (event.value === KEY_DELETE) delete_key();
  else if (event.value === KEY_THEN) then_key();
  else if (event.value === KEY_GO) start();
  else type_digit((event.value + 1) % 10);
}

function load_preset(place: number) {
  preset_place.value = place;
  for (const entry of QUEUE_PLACES) {
    queue_minutes.value[entry] = PRESETS[place * QUEUE_LENGTH_LIMIT + entry];
  }
  queue_length.value = PRESET_LENGTHS[place];
  awaits_next_entry.value = false;
}

function swipe(event: SwipeEvent) {
  if (!is_stopped.value) return;
  if (event.direction === "up") {
    buzz_seconds.value = buzz_seconds.value >= LONGEST_BUZZ_SECONDS ? 1 : buzz_seconds.value + 1;
  } else if (event.direction === "down") {
    buzz_seconds.value = buzz_seconds.value <= 1 ? LONGEST_BUZZ_SECONDS : buzz_seconds.value - 1;
  } else {
    const step = event.direction === "right" ? 1 : PRESET_COUNT - 1;
    load_preset((preset_place.value + step) % PRESET_COUNT);
  }
}

export function start() {
  if (!can_start.value) return;
  running_place.value = 0;
  rounds_done.value = 0;
  ends_at.value = now.value + queue_minutes.value[0] * MINUTE;
}

export function stop() {
  ends_at.value = null;
  rang_at.value = null;
  running_place.value = 0;
  rounds_done.value = 0;
  stop_vibrating();
}

function add_minute() {
  if (ends_at.value !== null) {
    ends_at.value = ends_at.value + MINUTE;
    vibrate(50);
  }
}

function ring(event: TimeEvent) {
  rang_at.value = event.time;
  ends_at.value = null;
  wake_screen();
  vibrate_pattern("alarm");
}

function start_next_entry(event: TimeEvent) {
  stop_vibrating();
  rang_at.value = null;
  const wraps = running_place.value + 1 >= queue_length.value;
  const next_place = wraps ? 0 : running_place.value + 1;
  const rounds = wraps ? rounds_done.value + 1 : rounds_done.value;
  if (rounds >= ROUND_LIMIT) {
    stop();
    return;
  }
  running_place.value = next_place;
  rounds_done.value = rounds;
  ends_at.value = event.time + queue_minutes.value[next_place] * MINUTE;
}

// ============================================================================
// Screen
// ============================================================================

function Pomodoro() {
  return (
    <Screen padding={8} gap={6} onSwipe={swipe}>
      <Row justify="space_between" height={44} padding={8} background="surface" radius={12}>
        <Label color="muted">{is_stopped.value ? `V${buzz_seconds.value}` : progress_text.value}</Label>
        {is_stopped.value && <Label long_mode="clip">{queue_text.value}</Label>}
      </Row>

      {is_stopped.value && (
        <ButtonMatrix
          grow={1}
          rows={KEYPAD_ROWS}
          highlighted={can_start.value ? ["Go"] : []}
          onClick={press_key}
        />
      )}

      {is_running.value && (
        <Card grow={1} align_items="center" justify="center" padding={0}>
          <Label font="huge">{format_duration(remaining.value, "mm:ss")}</Label>
        </Card>
      )}

      {is_ringing.value && (
        <Card grow={1} align_items="center" justify="center" gap={6} padding={0}>
          <Icon icon={icons.timer} size={48} color="warning" />
          <Label>Pomodoro</Label>
        </Card>
      )}

      {!is_stopped.value && (
        <Row gap={6} height={52}>
          <Button grow={2} height="100%" background="danger" onClick={stop}>
            Stop
          </Button>
          {is_running.value && (
            <Button grow={1} height="100%" onClick={add_minute}>
              +1
            </Button>
          )}
        </Row>
      )}
    </Screen>
  );
}

export default define_app({
  id: "pomodoro",
  name: "Pomodoro",
  group: "time",
  icon: icons.timer,
  screens: { main: Pomodoro },
  subscriptions: [at(ends_at, ring), at(next_entry_at, start_next_entry)],
  permissions: ["wake", "background"],
});
