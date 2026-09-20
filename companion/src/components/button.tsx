import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  type PressableProps,
  type StyleProp,
  type ViewStyle,
} from 'react-native';

import { ThemedText } from './themed-text';

import { Radius, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';

type ButtonProps = Omit<PressableProps, 'children' | 'style'> & {
  style?: StyleProp<ViewStyle>;
  title: string;
  variant?: 'primary' | 'secondary' | 'danger';
  busy?: boolean;
};

export function Button({ title, variant = 'primary', busy, disabled, style, ...rest }: ButtonProps) {
  const theme = useTheme();
  const background =
    variant === 'primary' ? theme.accent : variant === 'danger' ? theme.danger : theme.backgroundSelected;
  const color = variant === 'secondary' ? theme.text : theme.onAccent;
  const inactive = disabled || busy;

  return (
    <Pressable
      disabled={inactive}
      style={({ pressed }) => [
        styles.button,
        { backgroundColor: background, opacity: inactive ? 0.5 : pressed ? 0.8 : 1 },
        style,
      ]}
      {...rest}>
      {busy ? (
        <ActivityIndicator color={color} />
      ) : (
        <ThemedText type="smallBold" style={{ color }}>
          {title}
        </ThemedText>
      )}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  button: {
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: Spacing.three,
    paddingHorizontal: Spacing.four,
    borderRadius: Radius.medium,
    minHeight: 48,
  },
});
