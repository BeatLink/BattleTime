import * as DocumentPicker from 'expo-document-picker';
import { useEffect, useState } from 'react';
import { Platform, StyleSheet, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { Row, ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useWatch } from '@/hooks/use-watch';
import { backupSize, createBackup, parseBackup, restoreBackup, type Backup } from '@/protocol/backup';
import { readAbi } from '@/protocol/transfer';
import { addBackup, loadBackups, removeBackup, type StoredBackup } from '@/state/backups';
import { useTask } from '@/state/use-task';

const size = (bytes: number) => (bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} kB`);

const when = (iso: string) =>
  new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });

function exportInBrowser(backup: Backup) {
  const url = URL.createObjectURL(new Blob([JSON.stringify(backup)], { type: 'application/json' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = `waspos-backup-${backup.created.slice(0, 10)}.json`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export default function BackupScreen() {
  const { packageChannel, watchReady, watch, sendRaw } = useWatch();
  const task = useTask();
  const [stepLogs, setStepLogs] = useState(false);
  const [backups, setBackups] = useState<StoredBackup[]>([]);
  const [progress, setProgress] = useState<string | null>(null);
  const [restored, setRestored] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    void loadBackups().then(setBackups);
  }, []);

  const report = (done: number, total: number, file: string) =>
    setProgress(`${file}: ${size(done)} of ${size(total)}`);

  const restore = (backup: Backup) =>
    task.run('Restoring', async () => {
      await restoreBackup(packageChannel, backup, { abi: await readAbi(packageChannel), onProgress: report });
      setRestored(true);
      setProgress(null);
    });

  const chosen = backups.find((entry) => entry.id === selected);

  return (
    <Screen title="Backup">
      <Card>
        <ThemedText type="small" themeColor="textSecondary">
          A backup holds the watch&apos;s settings, alarms and app settings, and its step logs if you
          include them. Firmware and apps are not included, since they can be installed again.
        </ThemedText>
      </Card>

      <Card title="Back up this watch">
        <ToggleRow
          label="Include step logs"
          detail="About 0.5 kB for each day the watch has logged, so a long history takes a while"
          value={stepLogs}
          onValueChange={setStepLogs}
        />
        <Button
          title="Back up now"
          disabled={!watchReady}
          busy={task.busy === 'Backing up'}
          onPress={() =>
            void task.run('Backing up', async () => {
              const backup = await createBackup(packageChannel, { stepLogs, onProgress: report });
              setBackups(await addBackup(watch?.name ?? 'Watch', backup));
              setProgress(null);
            })
          }
        />
      </Card>

      {progress ? (
        <ThemedText type="small" themeColor="textSecondary">
          {progress}
        </ThemedText>
      ) : null}

      {restored ? (
        <Card>
          <ThemedText type="smallBold">Restored.</ThemedText>
          <ThemedText type="small" themeColor="textSecondary">
            The watch reads its settings and alarms when it starts, so restart it to use them.
          </ThemedText>
          <Button
            title="Restart the watch"
            onPress={() => {
              setRestored(false);
              void sendRaw('import machine; machine.reset()\r\n');
            }}
          />
        </Card>
      ) : null}

      <Card title="Saved on this device">
        {backups.length === 0 ? (
          <ThemedText type="small" themeColor="textSecondary">
            No backups yet.
          </ThemedText>
        ) : null}
        {backups.map((entry) => (
          <Row
            key={entry.id}
            label={when(entry.backup.created)}
            detail={`${entry.watch}, ${Object.keys(entry.backup.files).length} files, ${size(backupSize(entry.backup))}`}
            onPress={() => setSelected(entry.id === selected ? null : entry.id)}
          />
        ))}
        {Platform.OS === 'web' ? (
          <Button
            title="Load a backup file"
            variant="secondary"
            onPress={() =>
              void task.run('Loading', async () => {
                const picked = await DocumentPicker.getDocumentAsync({ type: 'application/json' });
                const asset = picked.canceled ? null : picked.assets?.[0];
                if (asset) {
                  const backup = parseBackup(await (await fetch(asset.uri)).text());
                  setBackups(await addBackup('Imported', backup));
                }
              })
            }
          />
        ) : null}
      </Card>

      {chosen ? (
        <Card title={when(chosen.backup.created)}>
          {Object.keys(chosen.backup.files).map((path) => (
            <ThemedText key={path} type="small" themeColor="textSecondary">
              {path}
            </ThemedText>
          ))}
          <View style={styles.actions}>
            <Button
              title="Restore to the watch"
              variant="danger"
              disabled={!watchReady}
              busy={task.busy === 'Restoring'}
              onPress={() => void restore(chosen.backup)}
            />
            {Platform.OS === 'web' ? (
              <Button title="Save as a file" variant="secondary" onPress={() => exportInBrowser(chosen.backup)} />
            ) : null}
            <Button
              title="Delete this backup"
              variant="secondary"
              onPress={() =>
                void removeBackup(chosen.id).then((list) => {
                  setBackups(list);
                  setSelected(null);
                })
              }
            />
          </View>
        </Card>
      ) : null}

      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}

const styles = StyleSheet.create({
  actions: {
    gap: Spacing.two,
    marginTop: Spacing.two,
  },
});
