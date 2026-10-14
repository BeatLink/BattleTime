import { describe, expect, it } from '@jest/globals';

import {
  dateOfStepSource,
  fileFromSlots,
  hourlySteps,
  readStepHistory,
  slotsFromFile,
  stepDayPath,
  stepDaySource,
} from './steps';

describe('the step log', () => {
  it('names a day as the watch and the sync do', () => {
    const day = new Date(2026, 0, 5);
    expect(stepDayPath(day)).toBe('/steps/2026/01-05');
    expect(stepDaySource(day)).toBe('steps/2026/01-05');
    expect(dateOfStepSource('steps/2026/01-05')).toEqual(day);
    expect(dateOfStepSource('/steps/2026/01-05')).toEqual(day);
    expect(dateOfStepSource('steps/trip')).toBeNull();
  });

  it('reads little-endian slots and writes them up to the last with steps', () => {
    expect(slotsFromFile(Uint8Array.from([5, 0, 0x2c, 0x01, 7]))).toEqual([5, 300]);
    expect(Array.from(fileFromSlots([5, 300, 0, 0]))).toEqual([5, 0, 0x2c, 0x01]);
    expect(slotsFromFile(fileFromSlots([0, 70000, 3]))).toEqual([0, 0xffff, 3]);
  });

  it('folds six-minute slots into hours', () => {
    const hours = hourlySteps(new Array(240).fill(1));
    expect(hours).toHaveLength(24);
    expect(hours.every((count) => count === 10)).toBe(true);
  });

  it('reads a run of days, today first, leaving days without a log empty', async () => {
    const asked: string[] = [];
    const history = await readStepHistory(
      async (date) => {
        asked.push(stepDayPath(date));
        return date.getDate() === 10 ? null : [10, 20];
      },
      3,
      new Date(2026, 9, 10, 15, 0),
    );
    expect(asked).toEqual(['/steps/2026/10-10', '/steps/2026/10-09', '/steps/2026/10-08']);
    expect(history[0]).toMatchObject({ slots: null, hours: [], total: 0 });
    expect(history[1].total).toBe(30);
    expect(history[1].hours[0]).toBe(30);
  });
});
