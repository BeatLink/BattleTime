import * as DocumentPicker from 'expo-document-picker';
import { useCallback, useEffect, useState } from 'react';
import { Platform, Pressable, StyleSheet, View } from 'react-native';

import { utf8Decode } from '@/ble/encoding';
import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Fonts, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import {
  deleteFile,
  joinPath,
  listDirectory,
  parentPath,
  readFile,
  readMemory,
  type WatchFile,
  type WatchMemory,
} from '@/protocol/files';
import { checkPath } from '@/protocol/packages';
import { readAbi, sendFile } from '@/protocol/transfer';
import { useTask } from '@/state/use-task';

// Longest file shown as text; anything bigger is only offered as a download.
const PREVIEW_LIMIT = 4096;

const size = (bytes: number) => (bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} kB`);

// Text is shown when it is mostly printable; a binary file shows only its size.
function asText(data: Uint8Array): string | null {
  const sample = data.subarray(0, PREVIEW_LIMIT);
  let printable = 0;
  for (const byte of sample) {
    if (byte === 9 || byte === 10 || byte === 13 || (byte >= 32 && byte < 127) || byte >= 128) {
      printable++;
    }
  }
  return sample.length === 0 || printable / sample.length > 0.95 ? utf8Decode(sample) : null;
}

// In a browser or the desktop app the file is handed to the browser's own download.
function saveInBrowser(name: string, data: Uint8Array) {
  const url = URL.createObjectURL(new Blob([data.slice().buffer]));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export default function FilesScreen() {
  const theme = useTheme();
  const { packageChannel, watchReady } = useWatch();
  const task = useTask();
  const [dir, setDir] = useState('');
  const [entries, setEntries] = useState<WatchFile[] | null>(null);
  const [opened, setOpened] = useState<{ file: WatchFile; data: Uint8Array } | null>(null);
  const [armed, setArmed] = useState<string | null>(null);
  const [memory, setMemory] = useState<WatchMemory | null>(null);

  const open = useCallback(
    (path: string) =>
      task.run(`Listing ${path || 'the flash'}`, async () => {
        setEntries(await listDirectory(packageChannel, path));
        setDir(path);
        setOpened(null);
        setArmed(null);
      }),
    [packageChannel, task],
  );

  useEffect(() => {
    if (watchReady) {
      void open('');
    }
    // List the top once per connection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watchReady]);

  const upload = () =>
    task.run('Sending the file', async () => {
      const picked = await DocumentPicker.getDocumentAsync({ copyToCacheDirectory: true });
      const asset = picked.canceled ? null : picked.assets?.[0];
      if (!asset) {
        return;
      }
      const path = checkPath(joinPath(dir, asset.name.replace(/[^A-Za-z0-9_.-]/g, '_')));
      const data = new Uint8Array(await (await fetch(asset.uri)).arrayBuffer());
      await sendFile(packageChannel, path, data, { abi: await readAbi(packageChannel) });
      setEntries(await listDirectory(packageChannel, dir));
    });

  const text = opened ? asText(opened.data) : null;

  return (
    <Screen title="Files">
      {!watchReady ? (
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            Connect to the watch to look at its files.
          </ThemedText>
        </Card>
      ) : null}

      {entries ? (
        <Card title={`/${dir}`}>
          {dir ? <Row label=".." detail="Up one folder" onPress={() => void open(parentPath(dir))} /> : null}
          {entries.length === 0 ? (
            <ThemedText type="small" themeColor="textSecondary">
              This folder is empty.
            </ThemedText>
          ) : null}
          {entries.map((entry) => (
            <Row
              key={entry.path}
              label={entry.directory ? `${entry.name}/` : entry.name}
              detail={entry.directory ? 'Folder' : size(entry.size)}
              onPress={() =>
                entry.directory
                  ? void open(entry.path)
                  : void task.run(`Reading ${entry.name}`, async () => {
                      setOpened({ file: entry, data: await readFile(packageChannel, entry.path) });
                      setArmed(null);
                    })
              }
            />
          ))}
          <Button title="Send a file here" variant="secondary" disabled={task.busy !== null} onPress={() => void upload()} />
        </Card>
      ) : null}

      {opened ? (
        <Card title={opened.file.name}>
          <ThemedText type="small" themeColor="textSecondary">
            {size(opened.data.length)}
            {opened.data.length > PREVIEW_LIMIT ? `, showing the first ${size(PREVIEW_LIMIT)}` : ''}
          </ThemedText>
          {text !== null ? (
            <ThemedText type="small" selectable style={styles.preview}>
              {text}
            </ThemedText>
          ) : (
            <ThemedText type="small" themeColor="textSecondary">
              Not a text file.
            </ThemedText>
          )}
          {Platform.OS === 'web' ? (
            <Button
              title="Download"
              variant="secondary"
              onPress={() => saveInBrowser(opened.file.name, opened.data)}
            />
          ) : null}
          <Pressable
            accessibilityRole="button"
            onPress={() => {
              if (armed !== opened.file.path) {
                setArmed(opened.file.path);
                return;
              }
              void task.run(`Deleting ${opened.file.name}`, async () => {
                await deleteFile(packageChannel, opened.file.path);
                setOpened(null);
                setArmed(null);
                setEntries(await listDirectory(packageChannel, dir));
              });
            }}>
            <ThemedText type="smallBold" style={[styles.delete, { color: theme.danger }]}>
              {armed === opened.file.path ? 'Tap again to delete it from the watch' : 'Delete'}
            </ThemedText>
          </Pressable>
        </Card>
      ) : null}

      {watchReady ? (
        <Card title="Memory">
          <Row
            label={memory ? `${size(memory.free)} free` : 'Heap'}
            detail={memory ? `${size(memory.alloc)} in use, after collecting garbage` : 'Collect garbage and read what is left'}
          />
          <Button
            title="Collect garbage"
            variant="secondary"
            disabled={task.busy !== null}
            onPress={() => void task.run('Collecting garbage', async () => setMemory(await readMemory(packageChannel)))}
          />
        </Card>
      ) : null}

      {task.busy ? (
        <View>
          <ThemedText type="small" themeColor="textSecondary">
            {task.busy}...
          </ThemedText>
        </View>
      ) : null}
      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}

const styles = StyleSheet.create({
  preview: {
    fontFamily: Fonts?.mono,
  },
  delete: {
    paddingVertical: Spacing.two,
  },
});
