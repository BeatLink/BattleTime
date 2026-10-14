import { afterEach, beforeEach, describe, expect, it, jest } from '@jest/globals';

import {
  CLOCK_CHECK_INTERVAL_MS,
  CLOCK_JUMP_THRESHOLD_MS,
  CLOCK_RESYNC_INTERVAL_MS,
  watchClockChanges,
} from './clock-changes';

// Clocks the test moves by hand: the wall clock can jump, the steady clock only moves forward.
function fakeClocks() {
  const clocks = { wall: 1_000_000, steady: 0, offset: 0 };
  return {
    clocks,
    sources: {
      wallClock: () => clocks.wall,
      steadyClock: () => clocks.steady,
      timeZoneOffset: () => clocks.offset,
    },
    // One check interval passes normally.
    tick() {
      clocks.wall += CLOCK_CHECK_INTERVAL_MS;
      clocks.steady += CLOCK_CHECK_INTERVAL_MS;
      jest.advanceTimersByTime(CLOCK_CHECK_INTERVAL_MS);
    },
  };
}

describe('watchClockChanges', () => {
  beforeEach(() => {
    jest.useFakeTimers();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it('stays quiet while the clock runs normally', () => {
    const { sources, tick } = fakeClocks();
    const onChange = jest.fn();
    const stop = watchClockChanges(onChange, sources);

    tick();
    tick();

    expect(onChange).not.toHaveBeenCalled();
    stop();
  });

  it('notices the clock being set', () => {
    const { clocks, sources, tick } = fakeClocks();
    const onChange = jest.fn();
    const stop = watchClockChanges(onChange, sources);

    clocks.wall += CLOCK_JUMP_THRESHOLD_MS + 1000;
    tick();

    expect(onChange).toHaveBeenCalledTimes(1);
    tick();
    expect(onChange).toHaveBeenCalledTimes(1);
    stop();
  });

  it('notices a sleep, when the wall clock runs on but the steady clock does not', () => {
    const { clocks, sources, tick } = fakeClocks();
    const onChange = jest.fn();
    const stop = watchClockChanges(onChange, sources);

    clocks.wall += 60 * 60 * 1000;
    tick();

    expect(onChange).toHaveBeenCalledTimes(1);
    stop();
  });

  it('notices a new time zone', () => {
    const { clocks, sources, tick } = fakeClocks();
    const onChange = jest.fn();
    const stop = watchClockChanges(onChange, sources);

    clocks.offset = 300;
    tick();

    expect(onChange).toHaveBeenCalledTimes(1);
    stop();
  });

  it('sets the watch again every few hours anyway', () => {
    const { sources, tick } = fakeClocks();
    const onChange = jest.fn();
    const stop = watchClockChanges(onChange, sources);

    for (let checks = 0; checks < CLOCK_RESYNC_INTERVAL_MS / CLOCK_CHECK_INTERVAL_MS; checks++) {
      tick();
    }

    expect(onChange).toHaveBeenCalledTimes(1);
    stop();
  });

  it('stops checking once stopped', () => {
    const { clocks, sources, tick } = fakeClocks();
    const onChange = jest.fn();
    const stop = watchClockChanges(onChange, sources);

    stop();
    clocks.wall += 60 * 60 * 1000;
    tick();

    expect(onChange).not.toHaveBeenCalled();
  });

  it('reads the real clocks by default', () => {
    const onChange = jest.fn();
    const stop = watchClockChanges(onChange);
    stop();
    expect(onChange).not.toHaveBeenCalled();
  });
});
