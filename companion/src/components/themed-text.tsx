import type { ComponentChildren, CSSProperties } from 'preact';

import { theme, type ThemeColor } from '@/constants/theme';

export type TextType = 'default' | 'title' | 'small' | 'smallBold' | 'subtitle' | 'code';

const TYPE_CLASS: Record<TextType, string> = {
  default: 'text-default',
  title: 'text-title',
  small: 'text-small',
  smallBold: 'text-small-bold',
  subtitle: 'text-subtitle',
  code: 'text-code',
};

export type ThemedTextProps = {
  type?: TextType;
  themeColor?: ThemeColor;
  // Keeps the text to one line, ending in an ellipsis.
  oneLine?: boolean;
  class?: string;
  style?: CSSProperties;
  children?: ComponentChildren;
};

// A paragraph in one of the app's text styles, coloured from the theme.
export function ThemedText({ type = 'default', themeColor, oneLine, class: extra, style, children }: ThemedTextProps) {
  const classes = [TYPE_CLASS[type], oneLine ? 'one-line' : null, extra].filter(Boolean).join(' ');
  return (
    <p class={classes} style={{ color: theme[themeColor ?? 'text'], ...style }}>
      {children}
    </p>
  );
}
