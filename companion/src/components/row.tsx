import { Pressable, StyleSheet, Switch, View, type ViewProps } from 'react-native';

import { ThemedText } from './themed-text';

import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';

type RowProps = ViewProps & {
  label: string;
  detail?: string;
  onPress?: () => void;
  right?: React.ReactNode;
};

// A label with optional detail text and a control on the right.
export function Row({ label, detail, onPress, right, style, ...rest }: RowProps) {
  const content = (
    <View style={[styles.row, style]} {...rest}>
      <View style={styles.text}>
        <ThemedText>{label}</ThemedText>
        {detail ? (
          <ThemedText type="small" themeColor="textSecondary">
            {detail}
          </ThemedText>
        ) : null}
      </View>
      {right}
    </View>
  );
  if (!onPress) {
    return content;
  }
  return (
    <Pressable onPress={onPress} style={({ pressed }) => pressed && styles.pressed}>
      {content}
    </Pressable>
  );
}

type ToggleRowProps = Omit<RowProps, 'right' | 'onPress'> & {
  value: boolean;
  onValueChange: (value: boolean) => void;
};

export function ToggleRow({ value, onValueChange, ...rest }: ToggleRowProps) {
  const theme = useTheme();
  return (
    <Row
      {...rest}
      right={
        <Switch
          value={value}
          onValueChange={onValueChange}
          trackColor={{ true: theme.accent, false: theme.backgroundSelected }}
          thumbColor={theme.text}
        />
      }
    />
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: Spacing.three,
    paddingVertical: Spacing.two,
  },
  text: {
    flex: 1,
    gap: Spacing.half,
  },
  pressed: {
    opacity: 0.7,
  },
});
