// Alarms, merged from wasp-os's alarm and InfiniTime's Alarm.
// From wasp-os: up to twelve alarms, each repeating on chosen weekdays, and a ten minute snooze.
// From InfiniTime: the 12 hour clock, a repeat button that cycles common patterns, the time until the
// next alarm, the side button stopping the ring, and a ring that stops itself after a minute.
// One `at` subscription follows whichever alarm is due first, so nothing ticks while waiting.

import { computed, signal } from "@preact/signals";
import {
  at,
  close,
  define_app,
  format_duration,
  format_number,
  format_time,
  go,
  now,
  persisted,
  range,
  settings,
  stop_vibrating,
  vibrate_pattern,
  wake_screen,
  back,
  next_time_of_day,
  EVERY_DAY,
  MINUTE,
  type Instant,
  type TimeEvent,
  type TimeOfDay,
} from "neotime";
import { TimePicker } from "neotime/components";
import { icons } from "neotime/icons";
import { Button, ClickArea, Column, Grid, Icon, Label, Row, Screen, Scroll, Switch } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const MAXIMUM_ALARMS = 12;

const SNOOZE_LENGTH = 10 * MINUTE;

const RING_LIMIT = MINUTE;

/** The day letters in mask order, Monday first, so a day's place is also its bit. */
const DAY_LETTERS = ["M", "T", "W", "T", "F", "S", "S"];

const MONDAY_TO_FRIDAY = 0b0011111;
const WEEKEND = 0b1100000;

/** The patterns the repeat button cycles through, in order; 0 rings once. */
const REPEAT_PRESETS = [0, EVERY_DAY, MONDAY_TO_FRIDAY, WEEKEND];
const REPEAT_PRESET_COUNT = 4;

/** One alarm: its time, its repeat days as a mask with Monday as bit 0 (0 rings once), and its switch. */
interface Alarm {
  hour: number;
  minute: number;
  days: number;
  enabled: boolean;
}

const alarm_count = persisted("alarm_count", 0);

/** Every alarm place; only the first `alarm_count` are in use. */
export const alarms = persisted<Alarm[]>("alarms", [
  { hour: 8, minute: 0, days: 0, enabled: false },
  { hour: 8, minute: 0, days: 0, enabled: false },
  { hour: 8, minute: 0, days: 0, enabled: false },
  { hour: 8, minute: 0, days: 0, enabled: false },
  { hour: 8, minute: 0, days: 0, enabled: false },
  { hour: 8, minute: 0, days: 0, enabled: false },
  { hour: 8, minute: 0, days: 0, enabled: false },
  { hour: 8, minute: 0, days: 0, enabled: false },
  { hour: 8, minute: 0, days: 0, enabled: false },
  { hour: 8, minute: 0, days: 0, enabled: false },
  { hour: 8, minute: 0, days: 0, enabled: false },
  { hour: 8, minute: 0, days: 0, enabled: false },
]);

/** When the next alarm or snooze rings, or null if none is set. */
export const next_ring_at = persisted<Instant | null>("next_ring_at", null);

/** Which alarm rings at `next_ring_at`. */
const next_ring_place = persisted("next_ring_place", -1);

export const snoozed_until = persisted<Instant | null>("snoozed_until", null);
const snoozed_place = persisted("snoozed_place", -1);

/** When the alarm on show started ringing, or null while none rings. */
export const rang_at = persisted<Instant | null>("rang_at", null);
const ringing_place = persisted("ringing_place", -1);

/** The alarm in the editor, or -1 for a new one not yet in the list. */
const editing_place = signal(-1);
export const editing_time = signal<TimeOfDay>({ hour: 8, minute: 0 });
export const editing_days = signal(0);

// ============================================================================
// Formatting
// ============================================================================

function format_time_of_day(hour: number, minute: number, uses_24_hour_clock: boolean): string {
  return uses_24_hour_clock
    ? `${format_number(hour, 2)}:${format_number(minute, 2)}`
    : `${format_number(hour % 12 === 0 ? 12 : hour % 12, 2)}:${format_number(minute, 2)} ${hour < 12 ? "AM" : "PM"}`;
}

function day_letter_if_set(days: number, place: number): string {
  return (days & (1 << place)) !== 0 ? DAY_LETTERS[place] : "-";
}

function repeat_name(days: number): string {
  return days === 0
    ? "Once"
    : days === EVERY_DAY
      ? "Every day"
      : days === MONDAY_TO_FRIDAY
        ? "Weekdays"
        : days === WEEKEND
          ? "Weekends"
          : `${day_letter_if_set(days, 0)}${day_letter_if_set(days, 1)}${day_letter_if_set(days, 2)}${day_letter_if_set(days, 3)}${day_letter_if_set(days, 4)}${day_letter_if_set(days, 5)}${day_letter_if_set(days, 6)}`;
}

// ============================================================================
// Derived values
// ============================================================================

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

const has_room = computed(() => alarm_count.value < MAXIMUM_ALARMS);

const next_alarm_summary = computed(() =>
  next_ring_at.value === null
    ? "No alarm set"
    : `${format_time(next_ring_at.value, "ddd")} ${format_time(next_ring_at.value, "clock")}, in ${format_duration(next_ring_at.value - now.value, "h:mm")}`,
);

const ring_ends_at = computed(() => (rang_at.value !== null ? rang_at.value + RING_LIMIT : null));

// ============================================================================
// Handlers
// ============================================================================

/** Points the subscription at whichever enabled alarm or snooze is due first. */
function schedule() {
  let soonest: Instant | null = snoozed_until.value;
  let soonest_place = snoozed_until.value !== null ? snoozed_place.value : -1;
  for (const place of range(MAXIMUM_ALARMS)) {
    if (place < alarm_count.value && alarms.value[place].enabled) {
      const candidate = next_time_of_day(
        now.value,
        { hour: alarms.value[place].hour, minute: alarms.value[place].minute },
        alarms.value[place].days === 0 ? EVERY_DAY : alarms.value[place].days,
      );
      if (soonest === null || candidate < soonest) {
        soonest = candidate;
        soonest_place = place;
      }
    }
  }
  next_ring_at.value = soonest;
  next_ring_place.value = soonest_place;
}

/** Flips an alarm's switch; a snooze of an alarm switched off is dropped. */
function toggle_enabled(place: number) {
  alarms.value[place].enabled = !alarms.value[place].enabled;
  if (!alarms.value[place].enabled && snoozed_place.value === place) {
    snoozed_until.value = null;
  }
  schedule();
}

export function open_new_alarm() {
  if (!has_room.value) return;
  editing_place.value = -1;
  editing_time.value = { hour: 8, minute: 0 };
  editing_days.value = 0;
  go("edit_time");
}

function open_alarm(place: number) {
  editing_place.value = place;
  editing_time.value = { hour: alarms.value[place].hour, minute: alarms.value[place].minute };
  editing_days.value = alarms.value[place].days;
  go("edit_time");
}

function toggle_day(place: number) {
  editing_days.value = editing_days.value ^ (1 << place);
}

/** Moves to the next preset, or to the first when the days are a pattern of their own. */
function cycle_repeat() {
  let next_preset = REPEAT_PRESETS[0];
  for (const preset_place of range(REPEAT_PRESET_COUNT)) {
    if (REPEAT_PRESETS[preset_place] === editing_days.value) {
      next_preset = REPEAT_PRESETS[(preset_place + 1) % REPEAT_PRESET_COUNT];
    }
  }
  editing_days.value = next_preset;
}

export function save_alarm() {
  const place = editing_place.value >= 0 ? editing_place.value : alarm_count.value;
  if (editing_place.value < 0) {
    alarm_count.value = alarm_count.value + 1;
  }
  alarms.value[place].hour = editing_time.value.hour;
  alarms.value[place].minute = editing_time.value.minute;
  alarms.value[place].days = editing_days.value;
  alarms.value[place].enabled = true;
  if (snoozed_place.value === place) {
    snoozed_until.value = null;
  }
  schedule();
  back();
  back();
}

function delete_alarm() {
  const place = editing_place.value;
  if (place >= 0) {
    // The alarms after the deleted one move up a place, and the last place goes back to its default.
    for (const each of range(MAXIMUM_ALARMS)) {
      if (each >= place) {
        const is_last = each + 1 === MAXIMUM_ALARMS;
        alarms.value[each].hour = is_last ? 8 : alarms.value[each + 1].hour;
        alarms.value[each].minute = is_last ? 0 : alarms.value[each + 1].minute;
        alarms.value[each].days = is_last ? 0 : alarms.value[each + 1].days;
        alarms.value[each].enabled = is_last ? false : alarms.value[each + 1].enabled;
      }
    }
    alarm_count.value = alarm_count.value - 1;
    if (snoozed_place.value === place) {
      snoozed_until.value = null;
    } else if (snoozed_place.value > place) {
      snoozed_place.value = snoozed_place.value - 1;
    }
    schedule();
  }
  back();
  back();
}

function ring(event: TimeEvent) {
  const place = next_ring_place.value;
  const is_snooze = snoozed_until.value !== null && event.time >= snoozed_until.value;
  ringing_place.value = place;
  rang_at.value = event.time;
  if (is_snooze) {
    snoozed_until.value = null;
  } else if (place >= 0 && alarms.value[place].days === 0) {
    alarms.value[place].enabled = false;
  }
  schedule();
  wake_screen();
  vibrate_pattern("alarm");
  go("ring");
}

function stop_ringing() {
  rang_at.value = null;
  ringing_place.value = -1;
  stop_vibrating();
}

export function dismiss() {
  stop_ringing();
  close();
}

export function snooze() {
  snoozed_until.value = now.value + SNOOZE_LENGTH;
  snoozed_place.value = ringing_place.value;
  stop_ringing();
  schedule();
  close();
}

function ring_timed_out() {
  if (rang_at.value !== null) dismiss();
}

// ============================================================================
// Screens
// ============================================================================

function AlarmRow(props: { place: number }) {
  const place = props.place;
  const enabled = alarms.value[place].enabled;
  return (
    <>
      {place < alarm_count.value && (
        <Row gap={6} padding={6} height={56} background="surface" radius={12}>
          <ClickArea grow={1} onClick={() => open_alarm(place)}>
            <Label color={enabled ? "text" : "muted"}>
              {format_time_of_day(alarms.value[place].hour, alarms.value[place].minute, uses_24_hour_clock.value)}
            </Label>
            <Label font="small" color="muted">
              {repeat_name(alarms.value[place].days)}
            </Label>
          </ClickArea>
          <Switch value={enabled} onChange={() => toggle_enabled(place)} />
        </Row>
      )}
    </>
  );
}

function AlarmList() {
  return (
    <Screen padding={4} gap={4}>
      <Row gap={6} height={28} padding={2}>
        <Icon icon={icons.alarm} size={22} color="accent" />
        <Label font="small" color="muted" long_mode="scroll">
          {next_alarm_summary.value}
        </Label>
      </Row>
      <Scroll grow={1} gap={6}>
        {range(MAXIMUM_ALARMS).map((place) => (
          <AlarmRow place={place} />
        ))}
        {has_room.value && (
          <Button height={48} onClick={open_new_alarm}>
            <Icon icon={icons.plus} />
          </Button>
        )}
      </Scroll>
    </Screen>
  );
}

function EditTime() {
  return (
    <Screen padding={4} gap={6}>
      <Column grow={1} justify="center">
        <TimePicker value={editing_time} />
      </Column>
      <Row gap={6} height={52}>
        <Button grow={1} height="100%" onClick={back}>
          <Icon icon={icons.close} />
        </Button>
        <Button grow={1} height="100%" background="accent" onClick={() => go("edit_days")}>
          <Icon icon={icons.next} />
        </Button>
      </Row>
    </Screen>
  );
}

function DayButton(props: { place: number }) {
  const is_set = (editing_days.value & (1 << props.place)) !== 0;
  return (
    <Button background={is_set ? "accent" : "surface"} onClick={() => toggle_day(props.place)}>
      {DAY_LETTERS[props.place]}
    </Button>
  );
}

function EditDays() {
  return (
    <Screen padding={4} gap={6}>
      <Label text_align="center">
        {format_time_of_day(editing_time.value.hour, editing_time.value.minute, uses_24_hour_clock.value)}
      </Label>
      <Grid columns={4} gap={4} height={96}>
        {range(7).map((place) => (
          <DayButton place={place} />
        ))}
      </Grid>
      <Button height={40} onClick={cycle_repeat}>
        {repeat_name(editing_days.value)}
      </Button>
      <Row gap={6} height={52} margin={0}>
        <Button grow={1} height="100%" background="danger" onClick={delete_alarm}>
          {editing_place.value >= 0 ? "Delete" : "Cancel"}
        </Button>
        <Button grow={1} height="100%" background="success" onClick={save_alarm}>
          <Icon icon={icons.check} />
        </Button>
      </Row>
    </Screen>
  );
}

function Ringing() {
  return (
    <Screen padding={4} gap={6} onButton={dismiss} onSwipe={() => {}}>
      <Column grow={1} align_items="center" justify="center" gap={4}>
        <Icon icon={icons.alarm} size={48} color="warning" />
        <Label font="large">{format_time(now.value, "clock")}</Label>
        <Label color="muted">Alarm</Label>
      </Column>
      <Row gap={6} height={64}>
        <Button grow={1} height="100%" background="accent" onClick={snooze}>
          Snooze
        </Button>
        <Button grow={1} height="100%" background="danger" onClick={dismiss}>
          <Icon icon={icons.stop} />
        </Button>
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "alarm",
  name: "Alarm",
  icon: icons.alarm,
  screens: { main: AlarmList, edit_time: EditTime, edit_days: EditDays, ring: Ringing },
  subscriptions: [at(next_ring_at, ring), at(ring_ends_at, ring_timed_out)],
  permissions: ["wake", "background"],
});
