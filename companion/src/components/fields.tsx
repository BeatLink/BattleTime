import type { TargetedEvent } from 'preact';
import { useState } from 'preact/hooks';

import { Button } from './button';
import { ThemedText } from './themed-text';

type Option<T> = { value: T; label: string };

type ChoiceRowProps<T> = {
  label: string;
  detail?: string;
  options: Option<T>[];
  value: T | undefined;
  onChange: (value: T) => void;
  disabled?: boolean;
};

// A label over a row of buttons, one of which is chosen.
export function ChoiceRow<T>({ label, detail, options, value, onChange, disabled }: ChoiceRowProps<T>) {
  return (
    <div class="field" role="group" aria-label={label}>
      <ThemedText>{label}</ThemedText>
      {detail ? (
        <ThemedText type="small" themeColor="textSecondary">
          {detail}
        </ThemedText>
      ) : null}
      <div class="options">
        {options.map((option) => {
          const selected = option.value === value;
          return (
            <button
              type="button"
              key={option.label}
              class={selected ? 'option selected' : 'option'}
              aria-pressed={selected}
              disabled={disabled}
              onClick={() => onChange(option.value)}>
              {option.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}

export type TextRowProps = {
  label: string;
  detail?: string;
  value: string;
  onChangeText?: (text: string) => void;
  onBlur?: () => void;
  // Called on Enter in a one-line box.
  onSubmit?: () => void;
  placeholder?: string;
  multiline?: boolean;
  // How many lines a multiline box shows before it scrolls.
  rows?: number;
  password?: boolean;
  inputMode?: 'text' | 'url' | 'numeric';
  autoCapitalize?: 'none' | 'sentences' | 'words';
  autoComplete?: string;
};

// A label with a text box under it.
export function TextRow({
  label,
  detail,
  value,
  onChangeText,
  onBlur,
  onSubmit,
  placeholder,
  multiline,
  rows,
  password,
  inputMode,
  autoCapitalize = 'none',
  autoComplete = 'off',
}: TextRowProps) {
  const shared = {
    class: 'input',
    value,
    placeholder,
    'aria-label': label,
    autoCapitalize,
    autoComplete,
    spellcheck: autoCapitalize === 'sentences',
    onInput: (event: TargetedEvent<HTMLInputElement | HTMLTextAreaElement>) => onChangeText?.(event.currentTarget.value),
    onBlur,
  };
  const submitOnEnter = (event: KeyboardEvent) => {
    if (event.key === 'Enter') onSubmit?.();
  };
  return (
    <label class="field">
      <ThemedText>{label}</ThemedText>
      {detail ? (
        <ThemedText type="small" themeColor="textSecondary">
          {detail}
        </ThemedText>
      ) : null}
      {multiline ? (
        <textarea {...shared} rows={rows ?? 3} />
      ) : (
        password ? (
          <input {...shared} type="password" onKeyDown={submitOnEnter} />
        ) : (
          <input {...shared} type="text" inputMode={inputMode} onKeyDown={submitOnEnter} />
        )
      )}
    </label>
  );
}

type SavedTextRowProps = Omit<TextRowProps, 'onChangeText' | 'onBlur' | 'onSubmit'> & {
  onSave: (text: string) => void;
};

// A text row that keeps its edit locally and saves it when the box loses focus or Enter is pressed.
export function SavedTextRow({ value, onSave, ...rest }: SavedTextRowProps) {
  const [text, setText] = useState(value);
  const save = () => {
    if (text !== value) {
      onSave(text);
    }
  };
  return <TextRow {...rest} value={text} onChangeText={setText} onBlur={save} onSubmit={save} />;
}

// An error from the watch or the network, with a way to put it away.
export function ErrorNote({ error, onDismiss }: { error: string | null; onDismiss?: () => void }) {
  if (!error) {
    return null;
  }
  return (
    <div class="stack">
      <ThemedText type="small" themeColor="danger">
        {error}
      </ThemedText>
      {onDismiss ? <Button title="Dismiss" variant="secondary" onClick={onDismiss} /> : null}
    </div>
  );
}
