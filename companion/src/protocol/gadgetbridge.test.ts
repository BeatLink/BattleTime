import { describe, expect, it } from '@jest/globals';

import { watchFaceNumber } from './gadgetbridge';

describe('watchFaceNumber', () => {
  it('gives the number the watch makes from a face id', () => {
    expect(watchFaceNumber('analog')).toBe(0x3114e811);
    expect(watchFaceNumber('digital')).toBe(708936699);
    expect(watchFaceNumber('')).toBe(0x011c9dc5);
  });
});
