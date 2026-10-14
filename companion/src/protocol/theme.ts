// The watch theme: a preset, then any of eight colours as "#rrggbb", sent in the settings message.
// The watch stores colours as RGB565, so one reads back as the nearest shade the screen shows.

import type { ThemeColorKey, ThemeColors } from './gadgetbridge';

export const THEME_COLOR_LABELS: Record<ThemeColorKey, string> = {
  background: 'Background',
  surface: 'Cards and buttons',
  text: 'Text',
  muted: 'Quiet text',
  accent: 'Accent',
  success: 'Success',
  warning: 'Warning',
  danger: 'Danger',
};

// Normalise #rrggbb or #rgb to lower-case #rrggbb, or null when the text is not a colour.
export function normaliseHex(text: string): string | null {
  let hex = text.trim().replace(/^#/, '');
  if (/^[0-9a-f]{3}$/i.test(hex)) {
    hex = [...hex].map((digit) => digit + digit).join('');
  }
  return /^[0-9a-f]{6}$/i.test(hex) ? `#${hex.toLowerCase()}` : null;
}

// The colours that differ from what the watch has, which is all a settings message needs to carry.
export function changedColors(current: ThemeColors, edited: ThemeColors): Partial<ThemeColors> {
  const changed: Partial<ThemeColors> = {};
  for (const key of Object.keys(edited) as ThemeColorKey[]) {
    if (edited[key].toLowerCase() !== current[key].toLowerCase()) {
      changed[key] = edited[key];
    }
  }
  return changed;
}

// Colours offered for a quick pick, before typing a hex code.
export const PALETTE = [
  '#ffffff', '#c0c0c0', '#808080', '#404040', '#000000', '#ff0000', '#ff8000', '#ffe300',
  '#80ff00', '#00c000', '#00ffc0', '#3be7ff', '#0080ff', '#3a3dff', '#8000ff', '#ff00c0',
];
