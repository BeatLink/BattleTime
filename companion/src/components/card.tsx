import { StyleSheet, type ViewProps } from 'react-native';

import { ThemedText } from './themed-text';
import { ThemedView } from './themed-view';

import { Radius, Spacing } from '@/constants/theme';

type CardProps = ViewProps & { title?: string };

// A rounded panel with an optional heading, used to group rows on every screen.
export function Card({ title, style, children, ...rest }: CardProps) {
  return (
    <ThemedView type="backgroundElement" style={[styles.card, style]} {...rest}>
      {title ? (
        <ThemedText type="smallBold" themeColor="textSecondary" style={styles.title}>
          {title.toUpperCase()}
        </ThemedText>
      ) : null}
      {children}
    </ThemedView>
  );
}

const styles = StyleSheet.create({
  card: {
    borderRadius: Radius.large,
    padding: Spacing.three,
    gap: Spacing.two,
  },
  title: {
    letterSpacing: 1,
    marginBottom: Spacing.one,
  },
});
