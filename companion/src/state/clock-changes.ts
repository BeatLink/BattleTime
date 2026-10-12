// Notices when the computer's or phone's clock changes under the app, so the watch can be set again.
//
// A clock jumps when it is set by hand, synced from the network, changed to a new time zone, or after a sleep.

// How often the clock is checked.
export const CLOCK_CHECK_INTERVAL_MS = 60 * 1000;

// How far the clock may wander from what elapsed before it counts as a jump.
export const CLOCK_JUMP_THRESHOLD_MS = 2000;

// How often the watch is set anyway, so its own drift never builds up.
export const CLOCK_RESYNC_INTERVAL_MS = 6 * 60 * 60 * 1000;

export type ClockSources = {
  wallClock: () => number;
  steadyClock: () => number;
  timeZoneOffset: () => number;
};

const realClocks: ClockSources = {
  wallClock: () => Date.now(),
  steadyClock: () => performance.now(),
  timeZoneOffset: () => new Date().getTimezoneOffset(),
};

// Calls `onChange` after a jump, a time zone change, or a long stretch without one; returns a function that stops it.
export function watchClockChanges(onChange: () => void, clocks: ClockSources = realClocks): () => void {
  let lastWall = clocks.wallClock();
  let lastSteady = clocks.steadyClock();
  let lastOffset = clocks.timeZoneOffset();
  let lastSync = lastSteady;

  const check = () => {
    const wall = clocks.wallClock();
    const steady = clocks.steadyClock();
    const offset = clocks.timeZoneOffset();

    const expectedWall = lastWall + (steady - lastSteady);
    const jumped = Math.abs(wall - expectedWall) > CLOCK_JUMP_THRESHOLD_MS;
    const zoneChanged = offset !== lastOffset;
    const due = steady - lastSync >= CLOCK_RESYNC_INTERVAL_MS;

    lastWall = wall;
    lastSteady = steady;
    lastOffset = offset;

    if (jumped || zoneChanged || due) {
      lastSync = steady;
      onChange();
    }
  };

  const timer = setInterval(check, CLOCK_CHECK_INTERVAL_MS);
  return () => clearInterval(timer);
}
