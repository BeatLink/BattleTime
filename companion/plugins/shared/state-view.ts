// How an app's synced state is shown and edited on its plugin's page, kept apart from the page so it is tested.
//
// State arrives as sync gives it (docs/sync.md): numbers, switches, lists, and records as objects of lists.

// ============================================================================
// What a plugin says about its app's values
// ============================================================================

// How one value reads; anything not named shows as a plain number. A `fixed` value is shown but not edited.
export type FieldKind = 'number' | 'switch' | 'time' | 'duration' | 'minutes' | 'fixed' | 'hidden';

// What sync says each value holds besides numbers and switches, by key.
export type SyncedKinds = Record<string, 'time' | 'duration' | 'color' | 'text'>;

// A record's note may name the value that counts how many of its entries are in use.
export type FieldNote = { label?: string; kind?: FieldKind; countedBy?: string };

// Keyed by the state's key, or `record.field` for a record's field.
export type FieldNotes = Record<string, FieldNote>;

export type Cell = { key: string; label: string; kind: FieldKind; value: unknown; index?: number };

export type Row = { label: string; cells: Cell[] };

// ============================================================================
// Reading
// ============================================================================

// "alarm_count" reads "Alarm count".
export function humanize(key: string): string {
  const words = key.replace(/[_.]+/g, ' ').trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

function kindOf(value: unknown, note: FieldNote | undefined, synced?: string): FieldKind {
  if (note?.kind) {
    return note.kind;
  }
  if (synced === 'time' || synced === 'duration') {
    return synced;
  }
  if (synced) {
    return 'fixed';
  }
  return typeof value === 'boolean' ? 'switch' : 'number';
}

// Each single value is a row of one cell; each list is a row of cells; a record is one row per entry.
export function rows(
  state: Record<string, unknown> | null | undefined,
  notes: FieldNotes = {},
  kinds: SyncedKinds = {},
): Row[] {
  const out: Row[] = [];
  for (const [key, value] of Object.entries(state ?? {})) {
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      const counted = notes[key]?.countedBy;
      const used = counted === undefined ? undefined : Number(state?.[counted]);
      out.push(...recordRows(key, value as Record<string, unknown>, notes, kinds, used));
      continue;
    }
    const note = notes[key];
    const kind = kindOf(Array.isArray(value) ? value[0] : value, note, kinds[key]);
    if (kind === 'hidden') {
      continue;
    }
    const label = note?.label ?? humanize(key);
    const cells = Array.isArray(value)
      ? value.map((item, index) => ({ key, label: `${index + 1}`, kind, value: item, index }))
      : [{ key, label, kind, value }];
    out.push({ label, cells });
  }
  return out;
}

function recordRows(
  record: string,
  fields: Record<string, unknown>,
  notes: FieldNotes,
  kinds: SyncedKinds,
  used?: number,
): Row[] {
  const shown = Object.entries(fields).filter(([field, value]) => {
    const key = `${record}.${field}`;
    return kindOf(Array.isArray(value) ? value[0] : value, notes[key], kinds[key]) !== 'hidden';
  });
  const length = Math.max(0, ...shown.map(([, value]) => (Array.isArray(value) ? value.length : 1)));
  const count = used !== undefined && Number.isInteger(used) ? Math.min(Math.max(used, 0), length) : length;
  const out: Row[] = [];
  for (let index = 0; index < count; index++) {
    out.push({
      label: `${notes[record]?.label ?? humanize(record)} ${index + 1}`,
      cells: shown.map(([field, value]) => {
        const key = `${record}.${field}`;
        const note = notes[key];
        const item = Array.isArray(value) ? value[index] : value;
        return {
          key,
          label: note?.label ?? humanize(field),
          kind: kindOf(item, note, kinds[key]),
          value: item,
          index: Array.isArray(value) ? index : undefined,
        };
      }),
    });
  }
  return out;
}

// A value as the page shows it.
export function display(cell: Cell): string {
  const value = cell.value;
  if (typeof value !== 'number') {
    return String(value);
  }
  switch (cell.kind) {
    case 'time':
      // Times are local milliseconds since 1970, so they are shown as UTC to keep the watch's wall clock.
      return value <= 0 ? 'never' : new Date(value).toISOString().slice(0, 16).replace('T', ' ');
    case 'duration':
      return `${Math.round(value / 1000)} s`;
    case 'minutes':
      return `${String(Math.floor(value / 60)).padStart(2, '0')}:${String(value % 60).padStart(2, '0')}`;
    default:
      return String(value);
  }
}

// ============================================================================
// Changing
// ============================================================================

// The changes to send for one edited cell, shaped as sync takes them: a list is sent whole.
export function change(state: Record<string, unknown>, cell: Cell, value: number | boolean): Record<string, unknown> {
  const [record, field] = cell.key.includes('.') ? cell.key.split('.', 2) : [cell.key, undefined];
  const current = field === undefined ? state[record] : (state[record] as Record<string, unknown>)?.[field];
  const next = Array.isArray(current) && cell.index !== undefined
    ? current.map((item, index) => (index === cell.index ? value : item))
    : value;
  return field === undefined ? { [record]: next } : { [record]: { [field]: next } };
}

// What the page's text box holds, as the number or switch the state keeps.
export function parse(cell: Cell, text: string): number | boolean | null {
  if (cell.kind === 'switch') {
    return text === 'true';
  }
  if (cell.kind === 'duration') {
    const seconds = Number(text.trim().replace(/\s*s$/, ''));
    return Number.isFinite(seconds) && seconds >= 0 ? Math.round(seconds * 1000) : null;
  }
  if (cell.kind === 'minutes') {
    const match = /^(\d{1,2}):(\d{2})$/.exec(text.trim());
    return match ? Number(match[1]) * 60 + Number(match[2]) : null;
  }
  const number = Number(text);
  return Number.isInteger(number) ? number : null;
}

// ============================================================================
// Steps
// ============================================================================

export type StepDay = { date: string; total: number };

// The days a steps source list holds, newest first, each with its total.
export function stepDays(days: { source: string; slots: number[] }[]): StepDay[] {
  return days
    .map(({ source, slots }) => {
      const [, year, monthDay] = source.split('/');
      return { date: `${year}-${monthDay}`, total: slots.reduce((sum, steps) => sum + steps, 0) };
    })
    .filter((day) => /^\d+-\d{2}-\d{2}$/.test(day.date))
    .sort((a, b) => b.date.localeCompare(a.date));
}

// ============================================================================
// Heart rate and battery
// ============================================================================

export type LogDay = { date: string; average: number; lowest: number; highest: number };

// The days a heart rate or battery source list holds, newest first, with the average, lowest and highest slot.
// A slot that is null, or equal to `empty`, had no reading; a day without any is left out.
export function logDays(days: { source: string; slots: (number | null)[] }[], empty?: number): LogDay[] {
  const out: LogDay[] = [];
  for (const { source, slots } of days) {
    const [, , year, monthDay] = source.split('/');
    const date = `${year}-${monthDay}`;
    const readings = slots.filter((slot): slot is number => typeof slot === 'number' && slot !== empty);
    if (!/^\d+-\d{2}-\d{2}$/.test(date) || readings.length === 0) {
      continue;
    }
    out.push({
      date,
      average: Math.round(readings.reduce((sum, value) => sum + value, 0) / readings.length),
      lowest: Math.min(...readings),
      highest: Math.max(...readings),
    });
  }
  return out.sort((a, b) => b.date.localeCompare(a.date));
}
