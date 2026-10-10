// Turns the watch's step log, one count per 6 minute slot, into what the Steps screen shows.

import { readSteps, type RequestChannel } from './requests';

// Minutes covered by one value in the log, set by TICK_PERIOD in wasp's steplogger.
export const SLOT_MINUTES = 6;
const SLOTS_PER_HOUR = 60 / SLOT_MINUTES;

export type StepDay = {
  date: Date;
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

// Read the last few days, today first.
export async function readStepHistory(
  channel: RequestChannel,
  days: number,
  today: Date = new Date(),
): Promise<StepDay[]> {
  const history: StepDay[] = [];
  for (let back = 0; back < days; back++) {
    const date = startOfDay(today, back);
    const reply = await readSteps(channel, date);
    const hours = reply.v ? hourlySteps(reply.v) : [];
    history.push({ date, hours, total: hours.reduce((sum, count) => sum + count, 0) });
  }
  return history;
}
