// The Alarm page's edits to the list of alarms, each giving the new list; kept apart from the page so they are tested.

import { MAX_ALARMS, NEW_ALARM, normaliseAlarm, type WatchAlarm } from '../../../src/protocol/alarms';

// "07:30" from a time field, or null for anything else.
export function parseClock(text: string): { hour: number; minute: number } | null {
  const match = /^(\d{1,2}):(\d{2})/.exec(text.trim());
  if (!match || Number(match[1]) > 23 || Number(match[2]) > 59) {
    return null;
  }
  return { hour: Number(match[1]), minute: Number(match[2]) };
}

function changeOne(alarms: WatchAlarm[], place: number, change: (alarm: WatchAlarm) => WatchAlarm): WatchAlarm[] {
  return alarms.map((alarm, at) => (at === place ? normaliseAlarm(change(alarm)) : alarm));
}

export function setTime(alarms: WatchAlarm[], place: number, text: string): WatchAlarm[] | null {
  const clock = parseClock(text);
  return clock ? changeOne(alarms, place, (alarm) => ({ ...alarm, ...clock })) : null;
}

export function toggleDay(alarms: WatchAlarm[], place: number, bit: number): WatchAlarm[] {
  return changeOne(alarms, place, (alarm) => ({ ...alarm, days: alarm.days ^ (1 << bit) }));
}

export function setEnabled(alarms: WatchAlarm[], place: number, enabled: boolean): WatchAlarm[] {
  return changeOne(alarms, place, (alarm) => ({ ...alarm, enabled }));
}

// A new alarm goes at the end, as on the watch, while there is room.
export function addAlarm(alarms: WatchAlarm[], room = MAX_ALARMS): WatchAlarm[] {
  return alarms.length < room ? [...alarms, NEW_ALARM] : alarms;
}

export function removeAlarm(alarms: WatchAlarm[], place: number): WatchAlarm[] {
  return alarms.filter((_, at) => at !== place);
}

// How many alarms the app's state has places for, at most twelve.
export function alarmRoom(state: Record<string, unknown> | null | undefined): number {
  const hours = (state?.alarms as { hour?: unknown[] } | undefined)?.hour;
  return Math.min(MAX_ALARMS, Array.isArray(hours) ? hours.length : 0);
}
