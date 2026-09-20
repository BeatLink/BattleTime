import { useState } from 'react';
import { StyleSheet, TextInput, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Fonts, Radius, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';

// Shows raw traffic in both directions and lets you type a line straight into the watch REPL.
export default function ConsoleScreen() {
  const theme = useTheme();
  const { console: entries, connection, sendRaw, clearConsole } = useWatch();
  const [draft, setDraft] = useState('');
  const connected = connection === 'connected';

  const submit = () => {
    const line = draft.trim();
    if (!line) return;
    sendRaw(`${line}\r\n`);
    setDraft('');
  };

  return (
    <Screen title="Console">
      <Card>
        {entries.length === 0 ? (
          <ThemedText type="small" themeColor="textSecondary">
            Nothing yet. Traffic appears here once a watch is connected.
          </ThemedText>
        ) : (
          entries.map((entry) => (
            <ThemedText
              key={entry.id}
              type="code"
              style={{ color: entry.direction === 'out' ? theme.accent : theme.text }}>
              {entry.direction === 'out' ? '> ' : '< '}
              {entry.text}
            </ThemedText>
          ))
        )}
      </Card>
      <View style={styles.inputRow}>
        <TextInput
          value={draft}
          onChangeText={setDraft}
          onSubmitEditing={submit}
          placeholder="Send a line to the REPL"
          placeholderTextColor={theme.textSecondary}
          autoCapitalize="none"
          autoCorrect={false}
          editable={connected}
          style={[
            styles.input,
            { color: theme.text, backgroundColor: theme.backgroundElement, fontFamily: Fonts.mono },
          ]}
        />
        <Button title="Send" onPress={submit} disabled={!connected} />
      </View>
      <Button title="Clear" variant="secondary" onPress={clearConsole} />
    </Screen>
  );
}

const styles = StyleSheet.create({
  inputRow: {
    flexDirection: 'row',
    gap: Spacing.two,
  },
  input: {
    flex: 1,
    borderRadius: Radius.medium,
    paddingHorizontal: Spacing.three,
    minHeight: 48,
  },
});
