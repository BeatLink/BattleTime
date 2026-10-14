import { describe, expect, it } from '@jest/globals';

import { appStateJson, applyAppStateChanges, decodeAppState, encodeAppState, type StateEntry } from './app-state';

const NONE = -(1n << 63n);

// A cut-down alarm state with a null time, a negative i16 and a duration.
const SAMPLE: StateEntry[] = [
  { key: 'alarm_count', kind: 'int', length: 0, width: 1, values: [2n] },
  { key: 'alarms.hour', kind: 'int', length: 3, width: 1, values: [7n, 9n, 8n] },
  { key: 'alarms.minute', kind: 'int', length: 3, width: 1, values: [0n, 30n, 0n] },
  { key: 'alarms.days', kind: 'int', length: 3, width: 1, values: [31n, 0n, 0n] },
  { key: 'alarms.enabled', kind: 'bool', length: 3, width: 1, values: [1n, 0n, 0n] },
  { key: 'next_ring_at', kind: 'instant', length: 0, width: 8, values: [NONE] },
  { key: 'snoozed_place', kind: 'int', length: 0, width: 2, values: [-1n] },
  { key: 'chosen', kind: 'duration', length: 0, width: 4, values: [600000n] },
];

const text = (value: string) => [...value].map((character) => character.charCodeAt(0));

// The bytes core/src/sync/state_file.rs encodes for SAMPLE, before and after the change below.
const RUST_BYTES = [
  ...text('NTST'), 1, 8,
  11, ...text('alarm_count'), 0, 0, 0, 1, 2,
  11, ...text('alarms.hour'), 0, 3, 0, 1, 7, 9, 8,
  13, ...text('alarms.minute'), 0, 3, 0, 1, 0, 30, 0,
  11, ...text('alarms.days'), 0, 3, 0, 1, 31, 0, 0,
  14, ...text('alarms.enabled'), 1, 3, 0, 1, 1, 0, 0,
  12, ...text('next_ring_at'), 2, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 128,
  13, ...text('snoozed_place'), 0, 0, 0, 2, 255, 255,
  6, ...text('chosen'), 3, 0, 0, 4, 192, 39, 9, 0,
];
// The same after alarm_count 3, hours 6, 9, 22 and the second alarm switched on.
const RUST_CHANGED = [
  ...text('NTST'), 1, 8,
  11, ...text('alarm_count'), 0, 0, 0, 1, 3,
  11, ...text('alarms.hour'), 0, 3, 0, 1, 6, 9, 22,
  13, ...text('alarms.minute'), 0, 3, 0, 1, 0, 30, 0,
  11, ...text('alarms.days'), 0, 3, 0, 1, 31, 0, 0,
  14, ...text('alarms.enabled'), 1, 3, 0, 1, 1, 1, 0,
  12, ...text('next_ring_at'), 2, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 128,
  13, ...text('snoozed_place'), 0, 0, 0, 2, 255, 255,
  6, ...text('chosen'), 3, 0, 0, 4, 192, 39, 9, 0,
];

describe('app state files', () => {
  it('encodes byte for byte as the core does, and reads back', () => {
    expect(Array.from(encodeAppState(SAMPLE))).toEqual(RUST_BYTES);
    expect(decodeAppState(Uint8Array.from(RUST_BYTES))).toEqual(SAMPLE);
  });

  it('reads the bytes the runtime writes', () => {
    const entries = decodeAppState(Uint8Array.from([...text('NTST'), 1, 1, 1, ...text('n'), 0, 0, 0, 2, 0xfe, 0xff]));
    expect(entries[0].values).toEqual([-2n]);
    expect(appStateJson(entries)).toEqual({ n: -2 });
  });

  it('gives records together and switches as booleans', () => {
    expect(appStateJson(SAMPLE)).toEqual({
      alarm_count: 2,
      alarms: { hour: [7, 9, 8], minute: [0, 30, 0], days: [31, 0, 0], enabled: [true, false, false] },
      next_ring_at: Number(NONE),
      snoozed_place: -1,
      chosen: 600000,
    });
  });

  it('applies changes as the core does, keeping the layout and the rest', () => {
    const changed = applyAppStateChanges(SAMPLE, {
      alarm_count: 3,
      alarms: { hour: [6, 9, 22], enabled: [true, true, false] },
    });
    expect(Array.from(encodeAppState(changed))).toEqual(RUST_CHANGED);
    // A null time read back as JSON and written again is unchanged.
    const again = applyAppStateChanges(SAMPLE, { next_ring_at: Number(NONE) });
    expect(Array.from(encodeAppState(again))).toEqual(RUST_BYTES);
  });

  it('refuses a change that would not load', () => {
    expect(() => applyAppStateChanges(SAMPLE, { alarm_count: 300 })).toThrow('cannot hold');
    expect(() => applyAppStateChanges(SAMPLE, { alarm_count: 'two' })).toThrow('whole numbers');
    expect(() => applyAppStateChanges(SAMPLE, { alarms: { hour: [6] } })).toThrow('list of 3');
    expect(() => applyAppStateChanges(SAMPLE, { alarms: { hour: 6 } })).toThrow('list of 3');
    expect(() => encodeAppState([{ ...SAMPLE[0], values: [128n] }])).toThrow('cannot hold');
  });

  it('refuses anything else', () => {
    const bytes = (...values: number[]) => Uint8Array.from(values);
    expect(() => decodeAppState(bytes())).toThrow("not an app's saved state");
    expect(() => decodeAppState(bytes(...text('NTST'), 2, 0))).toThrow("not an app's saved state");
    expect(() => decodeAppState(bytes(...text('NTST'), 1, 1, 1, 110, 0, 0, 0, 2, 0xfe))).toThrow('part way');
    expect(() => decodeAppState(bytes(...text('NTST'), 1, 1, 1, 110, 9, 0, 0, 2, 0xfe, 0xff))).toThrow('no known type');
    expect(() => decodeAppState(bytes(...text('NTST'), 1, 1, 1, 110, 0, 0, 0, 3, 0xfe, 0xff, 0))).toThrow('3 bytes');
    expect(() => decodeAppState(bytes(...text('NTST'), 1, 2))).toThrow('part way');
  });
});
