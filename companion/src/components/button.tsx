import type { CSSProperties } from 'preact';

import { Spinner } from './spinner';

type ButtonProps = {
  title: string;
  variant?: 'primary' | 'secondary' | 'danger';
  busy?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  style?: CSSProperties;
};

// A full-width button in the accent colour, a quieter grey or the danger red; a busy one shows a spinner.
export function Button({ title, variant = 'primary', busy, disabled, onClick, style }: ButtonProps) {
  return (
    <button type="button" class={`button ${variant}`} disabled={disabled || busy} onClick={onClick} style={style}>
      {busy ? <Spinner /> : title}
    </button>
  );
}
