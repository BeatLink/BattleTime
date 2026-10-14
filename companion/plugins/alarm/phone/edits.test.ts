import { describe, expect, it } from '@jest/globals';

import { addAlarm, alarmRoom, parseClock, removeAlarm, setEnabled, setTime, toggleDay } from './edits';

const TWO = [
  { hour: 7, minute: 0, days: 0x1f, enabled: true },
  { hour: 9, minute: 30, days: 0, enabled: false },
];

describe('the alarm page edits', () => {
  it('reads a time field', () => {
    expect(parseClock('07:05')).toEqual({ hour: 7, minute: 5 });
    expect(parseClock('7:05:00')).toEqual({ hour: 7, minute: 5 });
    expect(parseClock('24:00')).toBeNull();
    expect(parseClock('')).toBeNull();
  });

  it('changes one alarm and leaves the rest', () => {
    expect(setTime(TWO, 1, '22:15')?.[1]).toEqual({ hour: 22, minute: 15, days: 0, enabled: false });
    expect(setTime(TWO, 1, 'soon')).toBeNull();
    expect(toggleDay(TWO, 0, 6)[0].days).toBe(0x5f);
    expect(toggleDay(TWO, 0, 0)[0].days).toBe(0x1e);
    expect(setEnabled(TWO, 1, true)[1].enabled).toBe(true);
    expect(setEnabled(TWO, 1, true)[0]).toBe(TWO[0]);
  });

  it('adds while there is room and removes by place', () => {
    expect(addAlarm(TWO)).toHaveLength(3);
    expect(addAlarm(TWO, 2)).toBe(TWO);
    expect(removeAlarm(TWO, 0)).toEqual([TWO[1]]);
  });

  it('finds the room from the state', () => {
    expect(alarmRoom({ alarms: { hour: new Array(12).fill(8) } })).toBe(12);
    expect(alarmRoom({ alarms: { hour: new Array(20).fill(8) } })).toBe(12);
    expect(alarmRoom(null)).toBe(0);
  });
});
