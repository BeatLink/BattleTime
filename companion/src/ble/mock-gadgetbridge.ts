// A small stand-in for BattleTime's message handling, so the settings and slots screens work against the mock watch.
// It answers with the same JSON the firmware sends.

import { THEME_COLOR_KEYS, type SettingsReply, type ThemeColors, type WatchSettingValues } from '@/protocol/gadgetbridge';
import { normaliseHex } from '@/protocol/theme';

// Two of the watch's presets, enough to switch between; the rest answer with the dark colours.
const PRESETS: Record<string, ThemeColors> = {
  dark: {
    background: '#000000', surface: '#292c31', text: '#ffffff', muted: '#8c929c',
    accent: '#00aaa5', success: '#39be5a', warning: '#f7aa18', danger: '#e74539',
  },
  light: {
    background: '#ffffff', surface: '#e6e8eb', text: '#000000', muted: '#5a5f66',
    accent: '#00807c', success: '#2a9445', warning: '#c98a10', danger: '#c8352b',
  },
};
const THEMES = ['dark', 'light', 'ocean', 'forest', 'sunset', 'contrast'];

// The line the watch's remote app sends when a button is tapped, for simulating taps.
export function remoteTapLine(button: number): string {
  return JSON.stringify({ t: 'app', ch: 'remote', d: { button } });
}

export class MockGadgetbridge {
  private settings: WatchSettingValues = {
    brightness: 2,
    theme: 'dark',
    theme_colors: { ...PRESETS.dark },
    blank_after: 15,
    clock_24h: true,
    units: 'Metric',
    step_goal: 10000,
    always_on: false,
    chimes: 0,
    do_not_disturb: false,
    gadgetbridge_name: false,
  };
  private slots: { update: [number, number]; recovery: [number, number] } = {
    update: [0, 392 * 1024],
    recovery: [0x65000, 388 * 1024],
  };

  // Handle one line from the phone. Returns the replies, or null when the line is not a GB() call.
  handle(text: string): string[] | null {
    const match = /^GB\((.*)\)\s*$/.exec(text.trim()) ?? /^(\{.*\})\s*$/.exec(text.trim());
    if (!match) {
      return null;
    }
    let message: Record<string, unknown>;
    try {
      message = JSON.parse(match[1]);
    } catch {
      return [JSON.stringify({ t: 'error', msg: 'could not read the line' })];
    }
    switch (message.t) {
      case 'settings':
        return [this.applySettings(message)];
      case 'state':
        // The watch takes up the app's rewritten state and sends nothing back.
        return typeof message.app === 'string' && message.app.length > 0 && message.app.length <= 32
          ? []
          : [JSON.stringify({ t: 'error', msg: 'state needs an app id' })];
      case 'status':
        return [JSON.stringify({ t: 'status', bat: 76, mv: 3912, charging: false, steps: 4321 })];
      case 'slots':
        return [this.answerSlots(message)];
      case 'app':
        // The watch hands an app message to its app and sends nothing back.
        return [];
      default:
        // Messages the watch does not answer fall through to the transport's echo.
        return null;
    }
  }

  // Moves the slots the message names, refusing an overlap as the watch does; no other rule is checked.
  private answerSlots(message: Record<string, unknown>): string {
    const update = (message.update as [number, number] | undefined) ?? this.slots.update;
    const recovery = (message.recovery as [number, number] | undefined) ?? this.slots.recovery;
    const moves = 'update' in message || 'recovery' in message;
    if (update[0] < recovery[0] + recovery[1] && recovery[0] < update[0] + update[1]) {
      return JSON.stringify({ t: 'slots', err: 'the slots overlap' });
    }
    this.slots = { update, recovery };
    return JSON.stringify({
      t: 'slots',
      update,
      recovery,
      area: 0x100000,
      firmware: 388 * 1024,
      image: 171428,
      restart: moves,
    });
  }

  // Check everything first and apply nothing on a bad value, as the watch does.
  private applySettings(message: Record<string, unknown>): string {
    const next: WatchSettingValues = { ...this.settings, theme_colors: { ...this.settings.theme_colors } };
    if ('theme' in message && message.theme !== 'custom') {
      if (typeof message.theme !== 'string' || !THEMES.includes(message.theme)) {
        return JSON.stringify({ t: 'error', msg: `unknown theme ${String(message.theme)}` });
      }
      next.theme_colors = { ...(PRESETS[message.theme] ?? PRESETS.dark) };
    }
    const colors = (message.theme_colors ?? {}) as Record<string, unknown>;
    for (const key of THEME_COLOR_KEYS) {
      if (key in colors) {
        const hex = typeof colors[key] === 'string' ? normaliseHex(colors[key]) : null;
        if (!hex || !/^#/.test(String(colors[key]))) {
          return JSON.stringify({ t: 'error', msg: `${key} is not #rrggbb` });
        }
        next.theme_colors[key] = hex;
      }
    }
    if ('brightness' in message) {
      const level = Number(message.brightness);
      if (!(level >= 1 && level <= 3)) {
        return JSON.stringify({ t: 'error', msg: 'brightness is 1 to 3' });
      }
      next.brightness = level;
    }
    for (const key of ['blank_after', 'clock_24h', 'units', 'step_goal', 'always_on', 'chimes', 'do_not_disturb', 'gadgetbridge_name'] as const) {
      if (key in message) {
        (next as Record<string, unknown>)[key] = message[key];
      }
    }
    const preset = Object.entries(PRESETS).find(([, colours]) =>
      THEME_COLOR_KEYS.every((key) => colours[key] === next.theme_colors[key]),
    );
    next.theme = preset ? preset[0] : 'custom';
    this.settings = next;
    const reply: SettingsReply = { t: 'settings', ...next, themes: THEMES };
    return JSON.stringify(reply);
  }
}
