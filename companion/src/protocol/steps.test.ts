import { describe, expect, it } from '@jest/globals';

import {
  dateOfStepSource,
  daysSince1970,
  fileFromSlots,
  hourlySteps,
  mergeSlots,
  readStepHistory,
  slotsFromBlock,
  slotsFromFile,
  STEP_BLOCK_BYTES,
  stepBlockPath,
  stepDayPath,
  stepDaySource,
  writeSlotsIntoBlock,
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

  it('finds a day in its block of eight, counted from 1970 in local time', () => {
    expect(daysSince1970(new Date(1970, 0, 1, 23, 0))).toBe(0);
    expect(daysSince1970(new Date(2026, 0, 5))).toBe(20458);
    expect(stepBlockPath(new Date(2026, 0, 5))).toBe('/day_blocks/steps/2557');
    expect(stepBlockPath(new Date(2026, 0, 10))).toBe('/day_blocks/steps/2557');
    expect(stepBlockPath(new Date(2026, 0, 11))).toBe('/day_blocks/steps/2558');
  });

  it('reads a day from its block, flipped, up to the last slot with steps', () => {
    const block = new Uint8Array(STEP_BLOCK_BYTES).fill(0xff);
    const day = new Date(2026, 0, 5);
    writeSlotsIntoBlock(block, day, [0, 300, 0, 4]);
    // 2026-01-05 is the third day of its block, so its slots start 960 bytes in, flipped.
    expect(Array.from(block.subarray(960, 968))).toEqual([0xff, 0xff, 0xd3, 0xfe, 0xff, 0xff, 0xfb, 0xff]);
    expect(slotsFromBlock(block, day)).toEqual([0, 300, 0, 4]);
    expect(slotsFromBlock(block, new Date(2026, 0, 6))).toEqual([]);
  });

  it('merges a day older firmware kept as a file with its block, and has none when neither holds steps', () => {
    expect(mergeSlots([5, 0, 7], [0, 0, 0, 9])).toEqual([5, 0, 7, 9]);
    expect(mergeSlots(null, [0, 2])).toEqual([0, 2]);
    expect(mergeSlots([1], [])).toEqual([1]);
    expect(mergeSlots(null, [])).toBeNull();
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
