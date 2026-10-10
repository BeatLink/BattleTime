// The watch theme: eleven RGB565 colours, two bytes each, in the order wasp.py's Manager keeps them.

export const THEME_SLOTS = [
  'Bluetooth icon',
  'Scroll indicator',
  'Battery',
  'Status bar clock',
  'Notification icon',
  'Bright text',
  'Mid text',
  'Interface',
  'Highlight 1',
  'Highlight 2',
  'Contrast',
];

// The firmware's own defaults, from wasp.py.
export const DEFAULT_THEME = [
  0x7b, 0xef, 0x7b, 0xef, 0x7b, 0xef, 0xe7, 0x3c, 0x7b, 0xef, 0xff, 0xff, 0xbd, 0xb6, 0x39, 0xff, 0xff, 0x00,
  0xdd, 0xd0, 0x00, 0x0f,
];

export function themeColours(theme: number[]): number[] {
  const colours: number[] = [];
  for (let i = 0; i + 1 < theme.length; i += 2) {
    colours.push((theme[i] << 8) | theme[i + 1]);
  }
  return colours;
}

export function themeBytes(colours: number[]): number[] {
  return colours.flatMap((colour) => [(colour >> 8) & 0xff, colour & 0xff]);
}

// Widen RGB565 to #rrggbb, repeating the top bits so white stays white.
export function rgb565ToHex(colour: number): string {
  const r = (colour >> 11) & 0x1f;
  const g = (colour >> 5) & 0x3f;
  const b = colour & 0x1f;
  const hex = (value: number) => value.toString(16).padStart(2, '0');
  return `#${hex((r << 3) | (r >> 2))}${hex((g << 2) | (g >> 4))}${hex((b << 3) | (b >> 2))}`;
}

// Parse #rrggbb or #rgb into RGB565, or null when the text is not a colour.
export function hexToRgb565(text: string): number | null {
  let hex = text.trim().replace(/^#/, '');
  if (/^[0-9a-f]{3}$/i.test(hex)) {
    hex = [...hex].map((digit) => digit + digit).join('');
  }
  if (!/^[0-9a-f]{6}$/i.test(hex)) {
    return null;
  }
  const value = parseInt(hex, 16);
  const r = (value >> 16) & 0xff;
  const g = (value >> 8) & 0xff;
  const b = value & 0xff;
  return ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
}
