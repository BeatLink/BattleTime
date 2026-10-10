// A small stand-in for wasp/gadgetbridge.py, so the settings, alarm and steps
// screens work against the mock watch. It answers with the same JSON the
// firmware sends.

import type { Alarm, SettingsReply, WatchSettingValues } from '@/protocol/gadgetbridge';

const FACES: [string, string][] = [
  ['faces.clock.ClockApp', 'Clock'],
  ['faces.chrono.ChronoApp', 'Chrono'],
  ['faces.word_clock.WordClockApp', 'Word clock'],
];

// A made-up but plausible day: quiet nights, a walk in the morning and another after work.
function sampleDay(seed: number): number[] {
  return Array.from({ length: 240 }, (_, slot) => {
    const hour = slot / 10;
    const active = (hour > 7.5 && hour < 9) || (hour > 17 && hour < 18.5) ? 90 : hour > 7 && hour < 22 ? 12 : 0;
    return Math.round(active * (0.6 + ((slot * 37 + seed * 11) % 10) / 12));
  });
}

export class MockGadgetbridge {
  private settings: WatchSettingValues = {
    brightness: 2,
    notify_level: 2,
    units: 'Metric',
    blank_after: 15,
    clock_24h: true,
    step_goal: 10000,
    theme: [0x7b, 0xef, 0x7b, 0xef],
    face: FACES[0][0],
  };
  private alarms: Required<Alarm>[] = [{ h: 7, m: 30, on: true, rep: 31 }];

  // Handle one line from the phone. Returns the replies, or null when the line is not a GB() call.
  handle(text: string): string[] | null {
    const match = /^GB\((.*)\)\s*$/.exec(text.trim());
    if (!match) {
      return null;
    }
    let message: Record<string, unknown>;
    try {
      message = JSON.parse(match[1]);
    } catch {
      return [JSON.stringify({ t: 'error', msg: 'SyntaxError: invalid syntax' })];
    }
    switch (message.t) {
      case 'settings':
        return [this.applySettings(message)];
      case 'alarm':
        if (Array.isArray(message.d)) {
          this.alarms = (message.d as Alarm[]).map((alarm) => ({
            h: alarm.h,
            m: alarm.m,
            on: alarm.on ?? true,
            rep: alarm.rep ?? 0,
          }));
        }
        return [JSON.stringify({ t: 'alarm', d: this.alarms })];
      case 'steps': {
        const day = Number(message.d);
        const v = day % 5 === 0 ? null : sampleDay(day);
        return [JSON.stringify({ t: 'steps', y: message.y, m: message.m, d: message.d, v, now: 4321 })];
      }
      default:
        // Messages the watch does not answer fall through to the transport's echo.
        return null;
    }
  }

  private applySettings(message: Record<string, unknown>): string {
    for (const key of Object.keys(this.settings) as (keyof WatchSettingValues)[]) {
      if (key in message) {
        (this.settings as Record<string, unknown>)[key] = message[key];
      }
    }
    const reply: SettingsReply = { t: 'settings', ...this.settings, faces: FACES };
    return JSON.stringify(reply);
  }
}
