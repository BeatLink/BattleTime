import { useCallback, useEffect, useState } from 'react';
import { StyleSheet, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import type { WatchApp } from '@/protocol/packages';
import { resetQuickApps, writeQuickApps } from '@/protocol/quick-apps';
import {
  addQuickApp,
  describeQuickApp,
  loadQuickApps,
  quickAppChoices,
  removeQuickApp,
} from '@/state/quick-apps-form';
import { moveItem } from '@/state/remote-form';
import { useTask } from '@/state/use-task';

// Edits the apps a swipe left or right on the watch face steps through, and saves them to the watch.
export default function QuickAppsScreen() {
  const theme = useTheme();
  const { files, packages, watchReady } = useWatch();
  const task = useTask();
  // The ring as edited here, or null until it has been read from the watch.
  const [ring, setRing] = useState<string[] | null>(null);
  const [apps, setApps] = useState<WatchApp[]>([]);
  // Whether the ring on the watch comes from its file rather than its built-in default.
  const [fromFile, setFromFile] = useState(false);
  const [changed, setChanged] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  // Reads the ring and the installed apps from the watch, dropping any unsaved changes.
  const read = useCallback(async () => {
    const loaded = await loadQuickApps(await files(), await packages());
    setRing(loaded.ring);
    setApps(loaded.apps);
    setFromFile(loaded.fromFile);
    setChanged(false);
    setNote(null);
  }, [files, packages]);

  const load = () => task.run('Reading the ring', read);

  useEffect(() => {
    if (watchReady) {
      void load();
    }
    // Read once per connection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watchReady]);

  const edit = (next: string[]) => {
    setRing(next);
    setChanged(true);
    setNote(null);
  };

  const save = () =>
    task.run('Saving the ring', async () => {
      if (!ring) return;
      await writeQuickApps(await files(), ring);
      setFromFile(true);
      setChanged(false);
      setNote('Saved. The watch uses the new ring on its next swipe.');
    });

  const reset = () =>
    task.run('Going back to the default', async () => {
      await resetQuickApps(await files());
      await read();
      setNote('The watch is back on its default ring.');
    });

  const busy = task.busy !== null;
  const choices = ring ? quickAppChoices(ring, apps) : [];

  return (
    <Screen title="Quick apps">
      <Card title="Ring">
        <ThemedText type="small" themeColor="textSecondary">
          Swipe left or right on the watch face to step through these apps in order; a step past either
          end goes back to the face. The watch skips an app that is not installed.
        </ThemedText>
        {ring === null ? (
          <ThemedText type="small" themeColor="textSecondary">
            {watchReady ? 'Reading the ring from the watch…' : 'Connect a watch to see its ring.'}
          </ThemedText>
        ) : null}
        {ring !== null && !fromFile && !changed ? (
          <ThemedText type="small" themeColor="textSecondary">
            The watch is using its default ring.
          </ThemedText>
        ) : null}
        {ring?.length === 0 ? <ThemedText>No apps in the ring; swipes on the face do nothing.</ThemedText> : null}
        {ring?.map((id, index) => {
          const entry = describeQuickApp(id, apps);
          return (
            <View key={`${index}-${id}`} style={[styles.entry, { borderColor: theme.backgroundSelected }]}>
              <ThemedText>{`${index + 1}. ${entry.name}`}</ThemedText>
              {!entry.installed ? (
                <ThemedText type="small" themeColor="textSecondary">
                  Not installed, so the watch skips it.
                </ThemedText>
              ) : null}
              <View style={styles.actions}>
                <Button title="Up" variant="secondary" disabled={index === 0} onPress={() => edit(moveItem(ring, index, -1))} />
                <Button
                  title="Down"
                  variant="secondary"
                  disabled={index === ring.length - 1}
                  onPress={() => edit(moveItem(ring, index, 1))}
                />
                <Button title="Remove" variant="danger" onPress={() => edit(removeQuickApp(ring, index))} />
              </View>
            </View>
          );
        })}
      </Card>

      {ring !== null ? (
        <Card title="Add an app">
          {choices.length === 0 ? (
            <ThemedText type="small" themeColor="textSecondary">
              Every installed app is in the ring.
            </ThemedText>
          ) : null}
          {choices.map((app) => (
            <Row key={app.id} label={app.name} detail={app.id} onPress={() => edit(addQuickApp(ring, app.id))} />
          ))}
        </Card>
      ) : null}

      <Card title="On the watch">
        {note ? (
          <ThemedText type="small" themeColor="textSecondary">
            {note}
          </ThemedText>
        ) : null}
        <View style={styles.actions}>
          <Button title="Save" disabled={!watchReady || ring === null || !changed || busy} onPress={() => void save()} />
          <Button title="Read again" variant="secondary" disabled={!watchReady || busy} onPress={() => void load()} />
          <Button
            title="Use the default"
            variant="secondary"
            disabled={!watchReady || ring === null || !fromFile || busy}
            onPress={() => void reset()}
          />
        </View>
        {task.busy ? (
          <ThemedText type="small" themeColor="textSecondary">
            {`${task.busy}…`}
          </ThemedText>
        ) : null}
      </Card>

      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}

const styles = StyleSheet.create({
  entry: { gap: Spacing.two, paddingVertical: Spacing.two, borderTopWidth: StyleSheet.hairlineWidth },
  actions: { flexDirection: 'row', flexWrap: 'wrap', gap: Spacing.two },
});
