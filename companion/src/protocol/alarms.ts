// The alarm app's alarms, read from and written into its saved state (neotime/apps/alarm/app.tsx).
// It imports nothing, so the Alarm plugin's page bundles it as the screens and the command line use it.
//
// The app keeps twelve alarm places in a record, `alarms.hour`, `alarms.minute`, `alarms.days` and
// `alarms.enabled`, of which the first `alarm_count` are in use. Its other keys are the app's own.

// ============================================================================
// Alarms
// ============================================================================

// One alarm: `days` is a mask with Monday as bit 0 and Sunday as bit 6, and 0 rings once.
export type WatchAlarm = { hour: number; minute: number; days: number; enabled: boolean };

export const MAX_ALARMS = 12;

export const EVERY_DAY = 0x7f;
export const WEEKDAYS = 0x1f;
export const WEEKEND = 0x60;

// Days as the mask numbers them, Monday first.
export const DAY_NAMES = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

// A new alarm as the watch app starts one.
export const NEW_ALARM: WatchAlarm = { hour: 8, minute: 0, days: 0, enabled: true };

export function repeatLabel(days: number): string {
  if (days === 0) return 'Once';
  if (days === EVERY_DAY) return 'Every day';
  if (days === WEEKDAYS) return 'Weekdays';
  if (days === WEEKEND) return 'Weekends';
  return DAY_NAMES.filter((_, bit) => days & (1 << bit)).join(', ');
}

export function clockText(alarm: { hour: number; minute: number }): string {
  return `${String(alarm.hour).padStart(2, '0')}:${String(alarm.minute).padStart(2, '0')}`;
}

// Keeps an alarm inside what the watch app can show.
export function normaliseAlarm(alarm: WatchAlarm): WatchAlarm {
  const whole = (value: number, top: number) => Math.min(top, Math.max(0, Math.floor(Number(value) || 0)));
  return {
    hour: whole(alarm.hour, 23),
    minute: whole(alarm.minute, 59),
    days: whole(alarm.days, EVERY_DAY) & EVERY_DAY,
    enabled: Boolean(alarm.enabled),
  };
}

// ============================================================================
// The app's state
// ============================================================================

export class AlarmStateError extends Error {}

type AlarmRecord = { hour: number[]; minute: number[]; days: number[]; enabled: boolean[] };

function alarmRecord(state: Record<string, unknown> | null | undefined): AlarmRecord {
  const record = state?.alarms as Partial<AlarmRecord> | undefined;
  const fields = ['hour', 'minute', 'days', 'enabled'] as const;
  if (!record || fields.some((field) => !Array.isArray(record[field]))) {
    throw new AlarmStateError('The alarm app has not saved its alarms yet; open it on the watch once.');
  }
  return record as AlarmRecord;
}

// The alarms in use, in the app's order.
export function alarmsFromState(state: Record<string, unknown> | null | undefined): WatchAlarm[] {
  const record = alarmRecord(state);
  const count = Math.min(Math.max(0, Number(state?.alarm_count) || 0), record.hour.length);
  return Array.from({ length: count }, (_, place) =>
    normaliseAlarm({
      hour: record.hour[place],
      minute: record.minute[place],
      days: record.days[place],
      enabled: Boolean(record.enabled[place]),
    }),
  );
}

// ============================================================================
// When the next alarm rings
// ============================================================================

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

// The first instant after `after` at a local time of day on one of the mask's days, as the watch's `next_time_of_day` finds it.
// The companion sets the watch's time zone, so this computer's zone is the watch's.
export function nextTimeOfDay(after: number, hour: number, minute: number, days: number): number {
  const startOfDay = (instant: number) => new Date(instant).setHours(0, 0, 0, 0);
  const offset = (hour * 60 + minute) * MINUTE;
  let day = startOfDay(after);
  for (let tries = 0; tries < 8; tries++) {
    const candidate = day + offset;
    if (candidate > after && (days & (1 << ((new Date(candidate).getDay() + 6) % 7))) !== 0) {
      return candidate;
    }
    day = startOfDay(day + DAY + 2 * HOUR);
  }
  return day + offset;
}

const sameAlarm = (a: WatchAlarm, b: WatchAlarm) =>
  a.hour === b.hour && a.minute === b.minute && a.days === b.days && a.enabled === b.enabled;

// The app's own keys after its alarms change, as its `schedule` and delete set them: a snooze follows its alarm or is dropped, and the next ring is found again.
function scheduleChanges(state: Record<string, unknown>, before: WatchAlarm[], after: WatchAlarm[], now: number) {
  const changes: Record<string, unknown> = {};
  let snoozedUntil = typeof state.snoozed_until === 'number' ? state.snoozed_until : null;
  let snoozedPlace = Number(state.snoozed_place ?? -1);
  if (snoozedUntil !== null) {
    const snoozed = before[snoozedPlace];
    const kept = snoozed ? after.findIndex((alarm, place) => place === snoozedPlace && sameAlarm(alarm, snoozed)) : -1;
    const moved = kept >= 0 || !snoozed ? kept : after.findIndex((alarm) => sameAlarm(alarm, snoozed));
    if (moved < 0 || !after[moved].enabled) {
      snoozedUntil = null;
      changes.snoozed_until = null;
    } else if (moved !== snoozedPlace) {
      snoozedPlace = moved;
      changes.snoozed_place = moved;
    }
  }
  let soonest = snoozedUntil;
  let soonestPlace = snoozedUntil !== null ? snoozedPlace : -1;
  after.forEach((alarm, place) => {
    if (!alarm.enabled) return;
    const candidate = nextTimeOfDay(now, alarm.hour, alarm.minute, alarm.days === 0 ? EVERY_DAY : alarm.days);
    if (soonest === null || candidate < soonest) {
      soonest = candidate;
      soonestPlace = place;
    }
  });
  changes.next_ring_at = soonest;
  changes.next_ring_place = soonestPlace;
  // Keys the app's saved state lacks are left out, so an older app still takes the write.
  return Object.fromEntries(Object.entries(changes).filter(([key]) => key in state));
}

// The changes that give the state these alarms; places past the last alarm keep what they held, as the app leaves them.
// The app's next ring and any snooze are worked out again from `now`, since the app does that only when it is used.
export function alarmStateChanges(
  state: Record<string, unknown> | null | undefined,
  alarms: WatchAlarm[],
  now: number = Date.now(),
) {
  const record = alarmRecord(state);
  const places = record.hour.length;
  if (alarms.length > Math.min(places, MAX_ALARMS)) {
    throw new AlarmStateError(`The watch holds at most ${Math.min(places, MAX_ALARMS)} alarms.`);
  }
  const fresh = alarms.map(normaliseAlarm);
  const pick = <Value>(field: keyof WatchAlarm, kept: Value[]) =>
    kept.map((old, place) => (place < fresh.length ? (fresh[place][field] as Value) : old));
  return {
    ...scheduleChanges(state ?? {}, alarmsFromState(state), fresh, now),
    alarm_count: fresh.length,
    alarms: {
      hour: pick('hour', record.hour),
      minute: pick('minute', record.minute),
      days: pick('days', record.days),
      enabled: pick('enabled', record.enabled.map(Boolean)),
    },
  };
}
