import { describe, expect, it } from '@jest/globals';

import {
  alarmsFromState,
  alarmStateChanges,
  clockText,
  MAX_ALARMS,
  nextTimeOfDay,
  normaliseAlarm,
  repeatLabel,
} from './alarms';

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
      next_ring_place: 0,
      alarm_count: 1,
      alarms: { hour: [6, 9, 8, 8], minute: [15, 30, 0, 0], days: [0x60, 0, 0, 0], enabled: [true, false, false, false] },
    });
  });

  it('finds the next ring and keeps a snooze with its alarm, as the app would', () => {
    // Wednesday 14 October 2026, 06:00 here, which is the watch's zone.
    const now = new Date(2026, 9, 14, 6, 0).getTime();
    const at = (day: number, hour: number, minute = 0) => new Date(2026, 9, day, hour, minute).getTime();
    const full = { ...STATE, next_ring_at: null, snoozed_until: null, snoozed_place: -1 };
    const weekdays = { hour: 7, minute: 0, days: 31, enabled: true };
    const saturday = { hour: 9, minute: 30, days: 0x20, enabled: true };
    expect(alarmStateChanges(full, [saturday, weekdays], now)).toMatchObject({ next_ring_at: at(14, 7), next_ring_place: 1 });
    expect(alarmStateChanges(full, [saturday], now)).toMatchObject({ next_ring_at: at(17, 9, 30), next_ring_place: 0 });
    expect(alarmStateChanges(full, [], now)).toMatchObject({ next_ring_at: null, next_ring_place: -1 });

    // The first alarm was snoozed until 06:05; deleting the one before nothing keeps it, deleting it drops it.
    const snoozed = { ...full, snoozed_until: at(14, 6, 5), snoozed_place: 0 };
    const kept = alarmStateChanges(snoozed, [weekdays, saturday], now);
    expect(kept).toMatchObject({ next_ring_at: at(14, 6, 5), next_ring_place: 0 });
    expect(kept).not.toHaveProperty('snoozed_until');
    const moved = alarmStateChanges(snoozed, [saturday, weekdays], now);
    expect(moved).toMatchObject({ snoozed_place: 1, next_ring_at: at(14, 6, 5), next_ring_place: 1 });
    const dropped = alarmStateChanges(snoozed, [saturday], now);
    expect(dropped).toMatchObject({ snoozed_until: null, next_ring_at: at(17, 9, 30) });
    const switchedOff = alarmStateChanges(snoozed, [{ ...weekdays, enabled: false }], now);
    expect(switchedOff).toMatchObject({ snoozed_until: null, next_ring_at: null });
  });

  it("finds a time of day on the mask's days only, and never now itself", () => {
    const wednesday = new Date(2026, 9, 14, 7, 0).getTime();
    expect(nextTimeOfDay(wednesday, 7, 0, 0x7f)).toBe(new Date(2026, 9, 15, 7, 0).getTime());
    expect(nextTimeOfDay(wednesday, 8, 0, 1)).toBe(new Date(2026, 9, 19, 8, 0).getTime());
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
