import type { ComponentChildren } from 'preact';

import { ThemedText } from './themed-text';

type RowProps = {
  label: string;
  detail?: string;
  onClick?: () => void;
  right?: ComponentChildren;
};

// A label with optional detail text and a control on the right; with onClick the whole row is a button.
export function Row({ label, detail, onClick, right }: RowProps) {
  const content = (
    <>
      <span class="row-text">
        <ThemedText>{label}</ThemedText>
        {detail ? (
          <ThemedText type="small" themeColor="textSecondary">
            {detail}
          </ThemedText>
        ) : null}
      </span>
      {right}
    </>
  );
  if (!onClick) {
    return <div class="row">{content}</div>;
  }
  return (
    <button type="button" class="row" onClick={onClick}>
      {content}
    </button>
  );
}

type SwitchProps = {
  value: boolean;
  onValueChange: (value: boolean) => void;
  disabled?: boolean;
  label: string;
};

// An on and off switch.
export function Switch({ value, onValueChange, disabled, label }: SwitchProps) {
  return (
    <input
      type="checkbox"
      role="switch"
      class="switch"
      aria-label={label}
      checked={value}
      disabled={disabled}
      onChange={(event) => onValueChange(event.currentTarget.checked)}
    />
  );
}

type ToggleRowProps = Omit<RowProps, 'right' | 'onClick'> & {
  value: boolean;
  onValueChange: (value: boolean) => void;
  disabled?: boolean;
};

export function ToggleRow({ value, onValueChange, disabled, ...rest }: ToggleRowProps) {
  return (
    <Row {...rest} right={<Switch value={value} onValueChange={onValueChange} disabled={disabled} label={rest.label} />} />
  );
}
