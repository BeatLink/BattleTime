// What the Steps page draws from the synced step logs; kept apart from the page so it is tested.

import { dateOfStepSource, hourlySteps, SLOTS_PER_DAY, startOfDay, stepDaySource } from '../../../src/protocol/steps';

export type SyncedDay = { source: string; slots: number[] };

export type DayBar = { label: string; detail: string; total: number; logged: boolean };

export const DAYS_SHOWN = 14;

// The last fortnight, oldest first and ending today, with a day the watch has no log for as empty.
export function fortnight(days: SyncedDay[], today: Date, count = DAYS_SHOWN): DayBar[] {
  const bySource = new Map(days.filter((day) => dateOfStepSource(day.source)).map((day) => [day.source, day.slots]));
  return Array.from({ length: count }, (_, index) => {
    const date = startOfDay(today, count - 1 - index);
    const slots = bySource.get(stepDaySource(date));
    return {
      label: index === count - 1 ? 'Today' : date.toLocaleDateString(undefined, { weekday: 'narrow' }),
      detail: date.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'short' }),
      total: (slots ?? []).reduce((sum, steps) => sum + steps, 0),
      logged: slots !== undefined,
    };
  });
}

// Today's six-minute slots, every one of the day's 240, with those not logged yet as 0.
export function todaySlots(days: SyncedDay[], today: Date): number[] {
  const slots = days.find((day) => day.source === stepDaySource(today))?.slots ?? [];
  return Array.from({ length: SLOTS_PER_DAY }, (_, slot) => slots[slot] ?? 0);
}

export function todayHours(days: SyncedDay[], today: Date): number[] {
  return hourlySteps(todaySlots(days, today));
}

export type Bar = { x: number; y: number; width: number; height: number };

// Bars side by side across the width, scaled so the tallest fills the height; a bar with steps is at least 1 high.
export function bars(values: number[], width: number, height: number, gap = 1): Bar[] {
  const top = Math.max(1, ...values);
  const step = width / Math.max(1, values.length);
  return values.map((value, index) => {
    const tall = value > 0 ? Math.max(1, (value / top) * height) : 0;
    return { x: index * step + gap / 2, y: height - tall, width: Math.max(0.5, step - gap), height: tall };
  });
}
