// Colours follow the watch's own look: an accent on black with white text.
//
// Each colour is a CSS variable from src/styles.css, which switches between light and dark with the system.

export const theme = {
  text: 'var(--text)',
  textSecondary: 'var(--text-secondary)',
  background: 'var(--background)',
  backgroundElement: 'var(--background-element)',
  backgroundSelected: 'var(--background-selected)',
  accent: 'var(--accent)',
  onAccent: 'var(--on-accent)',
  success: 'var(--success)',
  warning: 'var(--warning)',
  danger: 'var(--danger)',
} as const;

export type ThemeColor = keyof typeof theme;

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
