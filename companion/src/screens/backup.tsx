import { useEffect, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import { backupSize, createBackup, parseBackup, restoreBackup, type Backup } from '@/protocol/backup';
import { pickFile, saveFile } from '@/services/device-files';
import { addBackup, loadBackups, removeBackup, type StoredBackup } from '@/state/backups';
import { useTask } from '@/state/use-task';

const size = (bytes: number) => (bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} kB`);

const when = (iso: string) =>
  new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });

function exportBackup(backup: Backup) {
  saveFile(`neotime-backup-${backup.created.slice(0, 10)}.json`, JSON.stringify(backup), 'application/json');
}

export default function BackupScreen() {
  const { files, watchReady, watch } = useWatch();
  const task = useTask();
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
      await restoreBackup(await files(), backup, { onProgress: report });
      setRestored(true);
      setProgress(null);
    });

  const chosen = backups.find((entry) => entry.id === selected);

  return (
    <Screen title="Backup">
      <Card>
        <ThemedText type="small" themeColor="textSecondary">
          A backup holds the watch&apos;s settings and what each app has saved. Firmware and apps are
          not included, since they can be installed again.
        </ThemedText>
      </Card>

      <Card title="Back up this watch">
        <Button
          title="Back up now"
          disabled={!watchReady}
          busy={task.busy === 'Backing up'}
          onClick={() =>
            void task.run('Backing up', async () => {
              const backup = await createBackup(await files(), { onProgress: report });
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
            The watch reads its settings when it starts, so restart it to use them.
          </ThemedText>
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
            onClick={() => setSelected(entry.id === selected ? null : entry.id)}
          />
        ))}
        <Button
          title="Load a backup file"
          variant="secondary"
          onClick={() =>
            void task.run('Loading', async () => {
              const picked = await pickFile('.json,application/json');
              if (picked) {
                const backup = parseBackup(new TextDecoder().decode(picked.data));
                setBackups(await addBackup('Imported', backup));
              }
            })
          }
        />
      </Card>

      {chosen ? (
        <Card title={when(chosen.backup.created)}>
          {Object.keys(chosen.backup.files).map((path) => (
            <ThemedText key={path} type="small" themeColor="textSecondary">
              {path}
            </ThemedText>
          ))}
          <div class="stack" style={{ marginTop: 8 }}>
            <Button
              title="Restore to the watch"
              variant="danger"
              disabled={!watchReady}
              busy={task.busy === 'Restoring'}
              onClick={() => void restore(chosen.backup)}
            />
            <Button title="Save as a file" variant="secondary" onClick={() => exportBackup(chosen.backup)} />
            <Button
              title="Delete this backup"
              variant="secondary"
              onClick={() =>
                void removeBackup(chosen.id).then((list) => {
                  setBackups(list);
                  setSelected(null);
                })
              }
            />
          </div>
        </Card>
      ) : null}

      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}
