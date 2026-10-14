import { describe, expect, it } from '@jest/globals';

import { change, display, humanize, logDays, parse, rows, stepDays } from './state-view';

const alarm = {
  alarm_count: 2,
  alarms: { hour: [7, 9, 0], minute: [0, 30, 0], enabled: [true, false, false] },
  next_ring_place: 0,
};

describe('reading state', () => {
  it('names keys in words', () => {
    expect(humanize('alarm_count')).toBe('Alarm count');
    expect(humanize('alarms.hour')).toBe('Alarms hour');
  });

  it('shows single values, lists and records, leaving hidden ones out', () => {
    const shown = rows(alarm, {
      alarms: { label: 'Alarm', countedBy: 'alarm_count' },
      next_ring_place: { kind: 'hidden' },
    });
    expect(shown.map((row) => row.label)).toEqual(['Alarm count', 'Alarm 1', 'Alarm 2']);
    expect(shown[2].cells.map((cell) => [cell.label, cell.kind, cell.value])).toEqual([
      ['Hour', 'number', 9],
      ['Minute', 'number', 30],
      ['Enabled', 'switch', false],
    ]);
    expect(rows({ laps: [1, 2] })[0].cells.map((cell) => cell.label)).toEqual(['1', '2']);
    expect(rows(null)).toEqual([]);
  });

  it('takes times and durations from what sync kept', () => {
    const shown = rows(
      { started_at: 5, laps: [1000], label: 2 },
      {},
      { started_at: 'time', laps: 'duration', label: 'text' },
    );
    expect(shown.map((row) => row.cells[0].kind)).toEqual(['time', 'duration', 'fixed']);
  });

  it('shows every entry of a record that nothing counts', () => {
    expect(rows(alarm).filter((row) => row.label.startsWith('Alarms'))).toHaveLength(3);
  });

  it('formats times, durations and minutes of the day', () => {
    const cell = (kind: 'time' | 'duration' | 'minutes' | 'number', value: unknown) => ({ key: 'k', label: 'k', kind, value });
    expect(display(cell('time', Date.UTC(2026, 9, 14, 7, 5)))).toBe('2026-10-14 07:05');
    expect(display(cell('time', 0))).toBe('never');
    expect(display(cell('duration', 90_000))).toBe('90 s');
    expect(display(cell('minutes', 7 * 60 + 5))).toBe('07:05');
    expect(display(cell('number', true))).toBe('true');
  });
});

describe('changing state', () => {
  it('sends a changed list whole and a single value alone', () => {
    const [, , second] = rows(alarm, { alarms: { countedBy: 'alarm_count' } });
    expect(change(alarm, second.cells[0], 10)).toEqual({ alarms: { hour: [7, 10, 0] } });
    expect(change(alarm, rows(alarm)[0].cells[0], 1)).toEqual({ alarm_count: 1 });
    const laps = { laps: [1, 2] };
    expect(change(laps, rows(laps)[0].cells[1], 5)).toEqual({ laps: [1, 5] });
  });

  it('reads what was typed as the state keeps it', () => {
    const cell = (kind: 'switch' | 'minutes' | 'number' | 'duration') => ({ key: 'k', label: 'k', kind, value: 0 });
    expect(parse(cell('duration'), '90 s')).toBe(90_000);
    expect(parse(cell('duration'), '-1')).toBeNull();
    expect(parse(cell('switch'), 'true')).toBe(true);
    expect(parse(cell('minutes'), '07:30')).toBe(450);
    expect(parse(cell('minutes'), '7.30')).toBeNull();
    expect(parse(cell('number'), '12')).toBe(12);
    expect(parse(cell('number'), '1.5')).toBeNull();
  });
});

describe('steps', () => {
  it('totals each day, newest first, and skips anything that is not a day', () => {
    expect(
      stepDays([
        { source: 'steps/2026/10-13', slots: [1, 2] },
        { source: 'steps/2026/10-14', slots: [5] },
        { source: 'steps/trip', slots: [] },
      ]),
    ).toEqual([
      { date: '2026-10-14', total: 5 },
      { date: '2026-10-13', total: 3 },
    ]);
  });
});

describe('heart rate and battery', () => {
  it('gives each day its average, lowest and highest, newest first, skipping slots without a reading', () => {
    expect(
      logDays(
        [
          { source: 'logs/heart/2026/10-13', slots: [0, 60, 0, 81] },
          { source: 'logs/heart/2026/10-14', slots: [70] },
          { source: 'logs/heart/2026/10-12', slots: [0, 0] },
        ],
        0,
      ),
    ).toEqual([
      { date: '2026-10-14', average: 70, lowest: 70, highest: 70 },
      { date: '2026-10-13', average: 71, lowest: 60, highest: 81 },
    ]);
  });

  it('keeps a battery at 0 % and skips the slots the watch was off', () => {
    expect(logDays([{ source: 'logs/battery/2026/10-14', slots: [null, 0, 40] }])).toEqual([
      { date: '2026-10-14', average: 20, lowest: 0, highest: 40 },
    ]);
    expect(logDays([{ source: 'logs/battery/x', slots: [5] }])).toEqual([]);
  });
});
