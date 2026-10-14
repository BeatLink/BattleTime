import { describe, expect, it } from '@jest/globals';

import { bars, fortnight, todayHours, todaySlots } from './chart';

const TODAY = new Date(2026, 9, 14, 15, 0);
const DAYS = [
  { source: 'steps/2026/10-01', slots: [1000] },
  { source: 'steps/2026/10-13', slots: [10, 20] },
  { source: 'steps/2026/10-14', slots: [0, 5, 7] },
  { source: 'steps/trip', slots: [] },
];

describe('the steps page', () => {
  it('gives the last fortnight oldest first, ending today', () => {
    const days = fortnight(DAYS, TODAY);
    expect(days).toHaveLength(14);
    expect(days[0]).toMatchObject({ total: 1000, logged: true });
    expect(days[1]).toMatchObject({ total: 0, logged: false });
    expect(days[12]).toMatchObject({ total: 30, logged: true });
    expect(days[13]).toMatchObject({ label: 'Today', total: 12, logged: true });
  });

  it("fills out today's slots and hours", () => {
    const slots = todaySlots(DAYS, TODAY);
    expect(slots).toHaveLength(240);
    expect(slots.slice(0, 4)).toEqual([0, 5, 7, 0]);
    expect(todayHours(DAYS, TODAY)[0]).toBe(12);
    expect(todaySlots([], TODAY).every((steps) => steps === 0)).toBe(true);
  });

  it('scales bars to the tallest', () => {
    const drawn = bars([0, 50, 100], 30, 100, 0);
    expect(drawn.map((bar) => bar.height)).toEqual([0, 50, 100]);
    expect(drawn.map((bar) => bar.x)).toEqual([0, 10, 20]);
    expect(drawn[2].y).toBe(0);
    expect(bars([1, 10000], 10, 100)[0].height).toBe(1);
  });
});
