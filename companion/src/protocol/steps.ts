// The watch's step log, one count per six-minute slot, and what the Steps screen makes of it.
// It imports nothing, so the Steps plugin's page bundles it as the screens and the command line use it.
//
// Each day holds the steps of each slot from midnight as a little-endian u16. The watch keeps eight days to
// a block, `/day_blocks/steps/<n>`, block n starting n * 8 days after 1970-01-01, each byte flipped so a slot
// never written reads 0 (neotime/services/slot_log/src/lib.rs). Older firmware kept each day as a file,
// `/steps/<year>/<MM>-<DD>`, ending after its last slot with steps; such a file stays beside the block.

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
// Blocks of days
// ============================================================================

export const DAYS_PER_STEP_BLOCK = 8;
// A block's bytes: a 4 KB sector less the file's header.
export const STEP_BLOCK_BYTES = 3872;
const DAY_BYTES = SLOTS_PER_DAY * 2;
const MILLISECONDS_PER_DAY = 86_400_000;

// The local day a date falls on, counted from 1970-01-01, as the watch counts its days.
export function daysSince1970(date: Date): number {
  return Math.round(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / MILLISECONDS_PER_DAY);
}

function dayInBlock(date: Date): number {
  const day = daysSince1970(date);
  return day - Math.floor(day / DAYS_PER_STEP_BLOCK) * DAYS_PER_STEP_BLOCK;
}

// The block holding a day, such as `/day_blocks/steps/2525`.
export function stepBlockPath(date: Date): string {
  return `/day_blocks/steps/${Math.floor(daysSince1970(date) / DAYS_PER_STEP_BLOCK)}`;
}

// A day's slots from its block, ending after the last slot with steps.
export function slotsFromBlock(block: Uint8Array, date: Date): number[] {
  const start = dayInBlock(date) * DAY_BYTES;
  const flipped = block.subarray(start, start + DAY_BYTES).map((byte) => byte ^ 0xff);
  const slots = slotsFromFile(flipped);
  while (slots.length > 0 && !slots[slots.length - 1]) slots.pop();
  return slots;
}

// Writes a day's slots into a block as the watch would, for the mock watch.
export function writeSlotsIntoBlock(block: Uint8Array, date: Date, slots: number[]) {
  const file = fileFromSlots(slots);
  const start = dayInBlock(date) * DAY_BYTES;
  file.forEach((byte, at) => {
    block[start + at] = byte ^ 0xff;
  });
}

// A day's slots from its own file, if older firmware left one, and from its block together, each slot having been
// written into one of them; null when the day has no file and no steps in its block.
export function mergeSlots(ownFile: number[] | null, fromBlock: number[]): number[] | null {
  if (ownFile === null && fromBlock.length === 0) {
    return null;
  }
  const own = ownFile ?? [];
  const length = Math.max(own.length, fromBlock.length);
  return Array.from({ length }, (_, slot) => (own[slot] ?? 0) + (fromBlock[slot] ?? 0));
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
