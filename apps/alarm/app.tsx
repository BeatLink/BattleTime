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

/** Every place in the alarm arrays, for loops and repeated rows. */
const ALARM_PLACES = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

const SNOOZE_LENGTH = 10 * MINUTE;

const RING_LIMIT = MINUTE;

/** The days in mask order, Monday first, so a day's place is also its bit. */
const DAY_PLACES = [0, 1, 2, 3, 4, 5, 6];
const DAY_LETTERS = ["M", "T", "W", "T", "F", "S", "S"];

const MONDAY_TO_FRIDAY = 0b0011111;
const WEEKEND = 0b1100000;

/** The patterns the repeat button cycles through, in order; 0 rings once. */
const REPEAT_PRESETS = [0, EVERY_DAY, MONDAY_TO_FRIDAY, WEEKEND];

const alarm_count = persisted("alarm_count", 0);
const alarm_hours = persisted<number[]>("alarm_hours", [8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8]);
const alarm_minutes = persisted<number[]>("alarm_minutes", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

/** Each alarm's repeat days as a mask with Monday as bit 0; 0 rings once. */
const alarm_days = persisted<number[]>("alarm_days", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

export const alarm_enabled = persisted<boolean[]>("alarm_enabled", [
  false, false, false, false, false, false, false, false, false, false, false, false,
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
// Array updates
// ============================================================================

function with_number(values: number[], place: number, value: number): number[] {
  return ALARM_PLACES.map((each) => (each === place ? value : values[each]));
}

function with_flag(values: boolean[], place: number, value: boolean): boolean[] {
  return ALARM_PLACES.map((each) => (each === place ? value : values[each]));
}

function without_number(values: number[], place: number, filler: number): number[] {
  return ALARM_PLACES.map((each) =>
    each < place ? values[each] : each + 1 < MAXIMUM_ALARMS ? values[each + 1] : filler,
  );
}

function without_flag(values: boolean[], place: number): boolean[] {
  return ALARM_PLACES.map((each) =>
    each < place ? values[each] : each + 1 < MAXIMUM_ALARMS ? values[each + 1] : false,
  );
}

// ============================================================================
// Handlers
// ============================================================================

/** Points the subscription at whichever enabled alarm or snooze is due first. */
function schedule() {
  let soonest: Instant | null = snoozed_until.value;
  let soonest_place = snoozed_until.value !== null ? snoozed_place.value : -1;
  for (const place of ALARM_PLACES) {
    if (place < alarm_count.value && alarm_enabled.value[place]) {
      const candidate = next_time_of_day(
        now.value,
        { hour: alarm_hours.value[place], minute: alarm_minutes.value[place] },
        alarm_days.value[place] === 0 ? EVERY_DAY : alarm_days.value[place],
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

function set_enabled(place: number, enabled: boolean) {
  alarm_enabled.value = with_flag(alarm_enabled.value, place, enabled);
  if (!enabled && snoozed_place.value === place) {
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
  editing_time.value = { hour: alarm_hours.value[place], minute: alarm_minutes.value[place] };
  editing_days.value = alarm_days.value[place];
  go("edit_time");
}

function toggle_day(place: number) {
  editing_days.value = editing_days.value ^ (1 << place);
}

function cycle_repeat() {
  let next = REPEAT_PRESETS[0];
  for (const place of [0, 1, 2, 3]) {
    if (editing_days.value === REPEAT_PRESETS[place]) {
      next = REPEAT_PRESETS[(place + 1) % 4];
    }
  }
  editing_days.value = next;
}

export function save_alarm() {
  const place = editing_place.value >= 0 ? editing_place.value : alarm_count.value;
  if (editing_place.value < 0) {
    alarm_count.value = alarm_count.value + 1;
  }
  alarm_hours.value = with_number(alarm_hours.value, place, editing_time.value.hour);
  alarm_minutes.value = with_number(alarm_minutes.value, place, editing_time.value.minute);
  alarm_days.value = with_number(alarm_days.value, place, editing_days.value);
  alarm_enabled.value = with_flag(alarm_enabled.value, place, true);
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
    alarm_hours.value = without_number(alarm_hours.value, place, 8);
    alarm_minutes.value = without_number(alarm_minutes.value, place, 0);
    alarm_days.value = without_number(alarm_days.value, place, 0);
    alarm_enabled.value = without_flag(alarm_enabled.value, place);
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
  } else if (place >= 0 && alarm_days.value[place] === 0) {
    alarm_enabled.value = with_flag(alarm_enabled.value, place, false);
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
  const enabled = alarm_enabled.value[place];
  return (
    <>
      {place < alarm_count.value && (
        <Row gap={6} padding={6} height={56} background="surface" radius={12}>
          <ClickArea grow={1} onClick={() => open_alarm(place)}>
            <Label color={enabled ? "text" : "muted"}>
              {format_time_of_day(alarm_hours.value[place], alarm_minutes.value[place], uses_24_hour_clock.value)}
            </Label>
            <Label font="small" color="muted">
              {repeat_name(alarm_days.value[place])}
            </Label>
          </ClickArea>
          <Switch value={enabled} onChange={(event) => set_enabled(place, event.value)} />
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
        {ALARM_PLACES.map((place) => (
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
        {DAY_PLACES.map((place) => (
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
