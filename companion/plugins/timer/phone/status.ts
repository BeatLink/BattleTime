// What the Timer page shows from the timer app's saved state (neotime/apps/timer/app.tsx); kept apart from the page so it is tested.
//
// Times are milliseconds since 1970 in the watch's local time, and a time that is not set reads as a
// large negative number, the runtime's null.

export const SECOND = 1000;
export const MINUTE = 60 * SECOND;
// The app's rollers go up to 99 minutes and 59 seconds.
export const LONGEST_TIMER = 99 * MINUTE + 59 * SECOND;

export type TimerState = {
  chosen_duration?: number;
  run_duration?: number;
  ends_at?: number;
  paused_with?: number;
  rang_at?: number;
};

export type TimerStatus =
  | { kind: 'idle' }
  | { kind: 'running'; endsAt: number; left: number }
  | { kind: 'paused'; left: number }
  | { kind: 'ringing'; since: number };

const isSet = (time: number | undefined): time is number => typeof time === 'number' && time > 0;

// The clock as the watch reads it: now, in local time, as milliseconds since 1970.
export function watchNow(date: Date = new Date()): number {
  return date.getTime() - date.getTimezoneOffset() * MINUTE;
}

export function timerStatus(state: TimerState | null | undefined, now: number): TimerStatus {
  if (isSet(state?.rang_at)) {
    return { kind: 'ringing', since: state.rang_at };
  }
  if (isSet(state?.ends_at)) {
    return { kind: 'running', endsAt: state.ends_at, left: Math.max(0, state.ends_at - now) };
  }
  if ((state?.paused_with ?? 0) > 0) {
    return { kind: 'paused', left: state!.paused_with! };
  }
  return { kind: 'idle' };
}

// "mm:ss", rounded up to the second as the app shows it.
export function durationText(ms: number): string {
  const seconds = Math.ceil(Math.max(0, ms) / SECOND);
  return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
}

// A local time as the watch's wall clock shows it, "HH:MM:SS".
export function clockOf(time: number): string {
  return new Date(time).toISOString().slice(11, 19);
}

export function statusText(status: TimerStatus): string {
  switch (status.kind) {
    case 'running':
      return `Running: ${durationText(status.left)} left, ends at ${clockOf(status.endsAt)}`;
    case 'paused':
      return `Paused with ${durationText(status.left)} left`;
    case 'ringing':
      return `Ringing since ${clockOf(status.since)}`;
    default:
      return 'Not running';
  }
}

// Minutes and seconds from the page's fields as the duration the app keeps, or null when out of range.
export function chosenDuration(minutes: string, seconds: string): number | null {
  const whole = (text: string) => (/^\d{1,2}$/.test(text.trim()) ? Number(text) : NaN);
  const ms = whole(minutes) * MINUTE + whole(seconds) * SECOND;
  return Number.isFinite(ms) && whole(seconds) < 60 && ms > 0 && ms <= LONGEST_TIMER ? ms : null;
}
