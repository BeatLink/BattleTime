import { useState } from 'react';
import { Pressable, StyleSheet, TextInput, View, type TextInputProps } from 'react-native';

import { Button } from './button';
import { ThemedText } from './themed-text';

import { Radius, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';

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
  const theme = useTheme();
  return (
    <View style={styles.choice}>
      <ThemedText>{label}</ThemedText>
      {detail ? (
        <ThemedText type="small" themeColor="textSecondary">
          {detail}
        </ThemedText>
      ) : null}
      <View style={styles.options}>
        {options.map((option) => {
          const selected = option.value === value;
          return (
            <Pressable
              key={option.label}
              disabled={disabled}
              accessibilityRole="button"
              accessibilityState={{ selected }}
              onPress={() => onChange(option.value)}>
              <View
                style={[
                  styles.option,
                  { backgroundColor: selected ? theme.accent : theme.backgroundSelected, opacity: disabled ? 0.5 : 1 },
                ]}>
                <ThemedText type="small" style={{ color: selected ? theme.onAccent : theme.text }}>
                  {option.label}
                </ThemedText>
              </View>
            </Pressable>
          );
        })}
      </View>
    </View>
  );
}

type TextRowProps = TextInputProps & { label: string; detail?: string };

// A label with a text box under it.
export function TextRow({ label, detail, style, ...rest }: TextRowProps) {
  const theme = useTheme();
  return (
    <View style={styles.choice}>
      <ThemedText>{label}</ThemedText>
      {detail ? (
        <ThemedText type="small" themeColor="textSecondary">
          {detail}
        </ThemedText>
      ) : null}
      <TextInput
        placeholderTextColor={theme.textSecondary}
        autoCapitalize="none"
        autoCorrect={false}
        style={[styles.input, { color: theme.text, backgroundColor: theme.backgroundSelected }, style]}
        {...rest}
      />
    </View>
  );
}

type SavedTextRowProps = Omit<TextRowProps, 'value' | 'onChangeText' | 'defaultValue'> & {
  value: string;
  onSave: (text: string) => void;
};

// A text row that keeps its edit locally and saves it when the box loses focus or is submitted.
export function SavedTextRow({ value, onSave, ...rest }: SavedTextRowProps) {
  const [text, setText] = useState(value);
  const save = () => {
    if (text !== value) {
      onSave(text);
    }
  };
  return <TextRow {...rest} value={text} onChangeText={setText} onBlur={save} onSubmitEditing={save} />;
}

// An error from the watch or the network, with a way to put it away.
export function ErrorNote({ error, onDismiss }: { error: string | null; onDismiss?: () => void }) {
  const theme = useTheme();
  if (!error) {
    return null;
  }
  return (
    <View style={styles.error}>
      <ThemedText type="small" style={{ color: theme.danger }}>
        {error}
      </ThemedText>
      {onDismiss ? <Button title="Dismiss" variant="secondary" onPress={onDismiss} /> : null}
    </View>
  );
}

const styles = StyleSheet.create({
  choice: {
    gap: Spacing.two,
    paddingVertical: Spacing.two,
  },
  options: {
    flexDirection: 'row',
    gap: Spacing.two,
    flexWrap: 'wrap',
  },
  option: {
    paddingVertical: Spacing.two,
    paddingHorizontal: Spacing.three,
    borderRadius: Radius.small,
    minHeight: 36,
    justifyContent: 'center',
  },
  input: {
    borderRadius: Radius.small,
    paddingHorizontal: Spacing.three,
    paddingVertical: Spacing.two,
    minHeight: 44,
  },
  error: {
    gap: Spacing.two,
  },
});
