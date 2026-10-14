import { describe, expect, it } from '@jest/globals';

import { chosenDuration, clockOf, durationText, MINUTE, statusText, timerStatus, watchNow } from './status';

const NONE = -9223372036854775808;
const NOON = Date.UTC(2026, 9, 14, 12, 0, 0);

describe('the timer page', () => {
  it('tells the timer state apart', () => {
    const idle = { chosen_duration: 10 * MINUTE, ends_at: NONE, paused_with: 0, rang_at: NONE };
    expect(timerStatus(idle, NOON)).toEqual({ kind: 'idle' });
    expect(timerStatus(null, NOON)).toEqual({ kind: 'idle' });
    expect(timerStatus({ ...idle, ends_at: NOON + 90500 }, NOON)).toEqual({ kind: 'running', endsAt: NOON + 90500, left: 90500 });
    expect(timerStatus({ ...idle, paused_with: 30000 }, NOON)).toEqual({ kind: 'paused', left: 30000 });
    expect(timerStatus({ ...idle, rang_at: NOON }, NOON)).toEqual({ kind: 'ringing', since: NOON });
  });

  it('says it in words', () => {
    expect(statusText({ kind: 'running', endsAt: NOON + 90500, left: 90500 })).toBe('Running: 01:31 left, ends at 12:01:30');
    expect(statusText({ kind: 'paused', left: 30000 })).toBe('Paused with 00:30 left');
    expect(statusText({ kind: 'ringing', since: NOON })).toBe('Ringing since 12:00:00');
    expect(statusText({ kind: 'idle' })).toBe('Not running');
    expect(durationText(99 * MINUTE + 59000)).toBe('99:59');
    expect(clockOf(NOON)).toBe('12:00:00');
  });

  it('reads the clock in local time', () => {
    const now = new Date(NOON);
    expect(watchNow(now)).toBe(NOON - now.getTimezoneOffset() * MINUTE);
  });

  it('takes a duration up to 99:59', () => {
    expect(chosenDuration('5', '30')).toBe(5 * MINUTE + 30000);
    expect(chosenDuration('99', '59')).toBe(99 * MINUTE + 59000);
    expect(chosenDuration('0', '0')).toBeNull();
    expect(chosenDuration('100', '0')).toBeNull();
    expect(chosenDuration('5', '60')).toBeNull();
    expect(chosenDuration('five', '0')).toBeNull();
  });
});
