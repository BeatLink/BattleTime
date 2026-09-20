// Colours follow the NeoTime design schema: an accent on black with white text.

import { Platform } from 'react-native';

export const Accent = '#00ACFF';

export const Colors = {
  light: {
    text: '#000000',
    textSecondary: '#60646C',
    background: '#FFFFFF',
    backgroundElement: '#F0F0F3',
    backgroundSelected: '#E0E1E6',
    accent: Accent,
    onAccent: '#FFFFFF',
    success: '#1DB954',
    warning: '#F5A623',
    danger: '#E5484D',
  },
  dark: {
    text: '#FFFFFF',
    textSecondary: '#B0B4BA',
    background: '#000000',
    backgroundElement: '#161719',
    backgroundSelected: '#26282C',
    accent: Accent,
    onAccent: '#000000',
    success: '#1DB954',
    warning: '#F5A623',
    danger: '#E5484D',
  },
} as const;

export type ThemeColor = keyof typeof Colors.light & keyof typeof Colors.dark;

export const Fonts = Platform.select({
  ios: {
    sans: 'system-ui',
    serif: 'ui-serif',
    rounded: 'ui-rounded',
    mono: 'ui-monospace',
  },
  default: {
    sans: 'normal',
    serif: 'serif',
    rounded: 'normal',
    mono: 'monospace',
  },
  web: {
    sans: 'Inter, system-ui, sans-serif',
    serif: 'Georgia, serif',
    rounded: 'system-ui, sans-serif',
    mono: 'ui-monospace, Menlo, Consolas, monospace',
  },
});

export const Spacing = {
  half: 2,
  one: 4,
  two: 8,
  three: 16,
  four: 24,
  five: 32,
  six: 64,
} as const;

export const Radius = {
  small: 8,
  medium: 12,
  large: 20,
} as const;

export const MaxContentWidth = 800;
