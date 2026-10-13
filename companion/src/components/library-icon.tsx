import { useMemo } from 'react';
import { Image, Pressable, StyleSheet, View } from 'react-native';

import { LIBRARY_ICON_NAMES } from '@/bundles/icon-names';
import { ThemedText } from '@/components/themed-text';
import { Radius, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { libraryIconUris } from '@/protocol/library-icons';

// A library icon as the watch draws it, tinted in the text colour, or its name where the bundle has no picture.
export function LibraryIcon({ name, size = 28 }: { name: string; size?: number }) {
  const theme = useTheme();
  const uris = useMemo(() => libraryIconUris(), []);
  const uri = uris[(LIBRARY_ICON_NAMES as readonly string[]).indexOf(name)];
  if (!uri) {
    return (
      <ThemedText type="small" style={{ width: size * 2 }} numberOfLines={1}>
        {name}
      </ThemedText>
    );
  }
  return <Image source={{ uri }} style={{ width: size, height: size, tintColor: theme.text }} accessibilityLabel={name} />;
}

// A grid of every library icon; the chosen one is outlined in the accent colour.
export function IconPicker({ value, onChange }: { value: string; onChange: (name: string) => void }) {
  const theme = useTheme();
  return (
    <View style={styles.grid}>
      {LIBRARY_ICON_NAMES.filter((name) => name !== 'none').map((name) => (
        <Pressable
          key={name}
          accessibilityLabel={name}
          onPress={() => onChange(name)}
          style={[
            styles.gridCell,
            { borderColor: name === value ? theme.accent : 'transparent', backgroundColor: theme.backgroundSelected },
          ]}>
          <LibraryIcon name={name} />
        </Pressable>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  grid: { flexDirection: 'row', flexWrap: 'wrap', gap: Spacing.one },
  gridCell: { padding: Spacing.one, borderWidth: 2, borderRadius: Radius.small },
});
