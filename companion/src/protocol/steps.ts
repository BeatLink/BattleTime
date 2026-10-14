// The watch's step log, one count per six-minute slot, and what the Steps screen makes of it.
// It imports nothing, so the Steps plugin's page bundles it as the screens and the command line use it.
//
// Each day is a file, `/steps/<year>/<MM>-<DD>`, holding the steps of each slot from midnight as a
// little-endian u16; it ends after the day's last slot with steps (neotime/services/steps/src/log.rs).

// ============================================================================
// The log's format
// ============================================================================

export const SLOT_MINUTES = 6;
export const SLOTS_PER_DAY = (24 * 60) / SLOT_MINUTES;
const SLOTS_PER_HOUR = 60 / SLOT_MINUTES;

const two = (number: number) => String(number).padStart(2, '0');

// The day's file on the watch, such as `/steps/2026/01-05`.
export function stepDayPath(date: Date): string {
  return `/steps/${date.getFullYear()}/${two(date.getMonth() + 1)}-${two(date.getDate())}`;
}

// The day's source in the companion's sync, such as `steps/2026/01-05`.
export function stepDaySource(date: Date): string {
  return stepDayPath(date).slice(1);
}

// The date a sync source or file path names, or null for anything else such as `steps/trip`.
export function dateOfStepSource(source: string): Date | null {
  const match = /^\/?steps\/(\d{4,5})\/(\d{2})-(\d{2})$/.exec(source);
  return match ? new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3])) : null;
}

export function slotsFromFile(bytes: Uint8Array): number[] {
  const slots: number[] = [];
  for (let at = 0; at + 1 < bytes.length && slots.length < SLOTS_PER_DAY; at += 2) {
    slots.push(bytes[at] | (bytes[at + 1] << 8));
  }
  return slots;
}

// The file the watch would write for these slots, which the mock watch serves.
export function fileFromSlots(slots: number[]): Uint8Array {
  let last = slots.length;
  while (last > 0 && !slots[last - 1]) last--;
  const bytes = new Uint8Array(last * 2);
  for (let slot = 0; slot < last; slot++) {
    const steps = Math.min(0xffff, Math.max(0, Math.round(slots[slot])));
    bytes[slot * 2] = steps & 0xff;
    bytes[slot * 2 + 1] = steps >> 8;
  }
  return bytes;
}

// ============================================================================
// What is shown
// ============================================================================

export type StepDay = {
  date: Date;
  // Steps in each six-minute slot from midnight, ending after the last slot with steps; null when the watch has no log.
  slots: number[] | null;
  // Steps in each hour of the day, midnight first; empty when the watch has no log.
  hours: number[];
  total: number;
};

export function hourlySteps(slots: number[]): number[] {
  const hours = new Array<number>(24).fill(0);
  slots.forEach((count, index) => {
    const hour = Math.floor(index / SLOTS_PER_HOUR);
    if (hour < 24) {
      hours[hour] += count;
    }
  });
  return hours;
}

export function startOfDay(date: Date, daysBack = 0): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() - daysBack);
}

export function stepDay(date: Date, slots: number[] | null): StepDay {
  const hours = slots ? hourlySteps(slots) : [];
  return { date, slots, hours, total: hours.reduce((sum, count) => sum + count, 0) };
}

// Reads one day's slots, or null when the watch has none for it.
export type StepDayReader = (date: Date) => Promise<number[] | null>;

// Reads the last few days, today first.
export async function readStepHistory(read: StepDayReader, days: number, today: Date = new Date()): Promise<StepDay[]> {
  const history: StepDay[] = [];
  for (let back = 0; back < days; back++) {
    const date = startOfDay(today, back);
    history.push(stepDay(date, await read(date)));
  }
  return history;
}
