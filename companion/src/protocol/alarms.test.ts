import { describe, expect, it } from '@jest/globals';

import { alarmsFromState, alarmStateChanges, clockText, MAX_ALARMS, normaliseAlarm, repeatLabel } from './alarms';

// The alarm app's state as sync gives it, cut to four places.
const STATE = {
  alarm_count: 2,
  alarms: { hour: [7, 9, 8, 8], minute: [0, 30, 0, 0], days: [31, 0, 0, 0], enabled: [true, false, false, false] },
  next_ring_place: 0,
};

describe('alarms', () => {
  it('reads the places in use', () => {
    expect(alarmsFromState(STATE)).toEqual([
      { hour: 7, minute: 0, days: 31, enabled: true },
      { hour: 9, minute: 30, days: 0, enabled: false },
    ]);
    expect(alarmsFromState({ ...STATE, alarm_count: 99 })).toHaveLength(4);
  });

  it('asks for the app to be opened once when it has saved nothing', () => {
    expect(() => alarmsFromState(null)).toThrow('open it on the watch once');
    expect(() => alarmsFromState({ alarm_count: 0 })).toThrow('open it on the watch once');
  });

  it('writes every place, keeping those past the last alarm, and nothing else', () => {
    const changes = alarmStateChanges(STATE, [{ hour: 6, minute: 15, days: 0x60, enabled: true }]);
    expect(changes).toEqual({
      alarm_count: 1,
      alarms: { hour: [6, 9, 8, 8], minute: [15, 30, 0, 0], days: [0x60, 0, 0, 0], enabled: [true, false, false, false] },
    });
  });

  it('refuses more alarms than the app has places', () => {
    const one = { hour: 1, minute: 1, days: 0, enabled: true };
    expect(() => alarmStateChanges(STATE, new Array(5).fill(one))).toThrow('at most 4');
    const twelve = { alarm_count: 0, alarms: { hour: new Array(20).fill(0), minute: new Array(20).fill(0), days: new Array(20).fill(0), enabled: new Array(20).fill(false) } };
    expect(() => alarmStateChanges(twelve, new Array(MAX_ALARMS + 1).fill(one))).toThrow(`at most ${MAX_ALARMS}`);
  });

  it('keeps values inside what the app shows', () => {
    expect(normaliseAlarm({ hour: 25, minute: -3, days: 0xff, enabled: true })).toEqual({ hour: 23, minute: 0, days: 0x7f, enabled: true });
    expect(clockText({ hour: 7, minute: 5 })).toBe('07:05');
  });

  it('names repeat patterns', () => {
    expect(repeatLabel(0)).toBe('Once');
    expect(repeatLabel(0x7f)).toBe('Every day');
    expect(repeatLabel(0x1f)).toBe('Weekdays');
    expect(repeatLabel(0x60)).toBe('Weekends');
    expect(repeatLabel(0b101)).toBe('Mon, Wed');
  });
});
