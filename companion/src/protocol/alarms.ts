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

// The changes that give the state these alarms; places past the last alarm keep what they held, as the app leaves them.
export function alarmStateChanges(state: Record<string, unknown> | null | undefined, alarms: WatchAlarm[]) {
  const record = alarmRecord(state);
  const places = record.hour.length;
  if (alarms.length > Math.min(places, MAX_ALARMS)) {
    throw new AlarmStateError(`The watch holds at most ${Math.min(places, MAX_ALARMS)} alarms.`);
  }
  const fresh = alarms.map(normaliseAlarm);
  const pick = <Value>(field: keyof WatchAlarm, kept: Value[]) =>
    kept.map((old, place) => (place < fresh.length ? (fresh[place][field] as Value) : old));
  return {
    alarm_count: fresh.length,
    alarms: {
      hour: pick('hour', record.hour),
      minute: pick('minute', record.minute),
      days: pick('days', record.days),
      enabled: pick('enabled', record.enabled.map(Boolean)),
    },
  };
}
