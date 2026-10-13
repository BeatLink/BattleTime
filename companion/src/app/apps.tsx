import * as DocumentPicker from 'expo-document-picker';
import { useRouter } from 'expo-router';
import { useCallback, useEffect, useState } from 'react';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import { appDetail, installImage, memoryText, type WatchApp } from '@/protocol/packages';
import { useTask } from '@/state/use-task';

// Lists the apps and faces on the watch, installs a compiled image, and hides or removes one.
export default function AppsScreen() {
  const theme = useTheme();
  const router = useRouter();
  const { files, packages, watchReady } = useWatch();
  const task = useTask();
  const [apps, setApps] = useState<WatchApp[] | null>(null);
  const [room, setRoom] = useState<number | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [armed, setArmed] = useState(false);

  // Lists the apps, with the memory each needs and the most one may have.
  const list = useCallback(async () => {
    const listing = await (await packages()).listing();
    setApps(listing.apps);
    setRoom(listing.room);
  }, [packages]);

  const refresh = useCallback(() => task.run('Listing apps', list), [list, task]);

  useEffect(() => {
    if (watchReady) {
      void refresh();
    }
    // List once per connection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watchReady]);

  const install = () =>
    task.run('Installing', async () => {
      const picked = await DocumentPicker.getDocumentAsync({ copyToCacheDirectory: true });
      const asset = picked.canceled ? null : picked.assets?.[0];
      if (!asset) {
        return;
      }
      const image = new Uint8Array(await (await fetch(asset.uri)).arrayBuffer());
      const header = await installImage(await files(), await packages(), image, (step) =>
        setProgress(`${step.file}: ${Math.round((step.sent / Math.max(1, step.total)) * 100)}%`),
      );
      setProgress(`Installed ${header.name}.`);
      await list();
    });

  const chosen = apps?.find((app) => app.id === selected) ?? null;

  const act = (label: string, action: () => Promise<void>) =>
    task.run(label, async () => {
      try {
        await action();
      } finally {
        setArmed(false);
        await list();
      }
    });

  return (
    <Screen title="Apps">
      <Card>
        <ThemedText type="small" themeColor="textSecondary">
          Apps and faces are compiled by the NeoTime compiler into .ntapp images, and the shared
          library into a .ntlib. Installing one sends it to the watch, which checks it before using
          it. Removing an app also deletes what it saved. The watch works out how much memory each app
          needs and refuses to turn on one that would not fit.
        </ThemedText>
        <Button title="Set up this watch" variant="secondary" onPress={() => router.push('/setup')} />
      </Card>

      <Card title="On the watch">
        {room !== null ? (
          <ThemedText type="small" themeColor="textSecondary">
            Each app may use up to {memoryText(room)} while it is open.
          </ThemedText>
        ) : null}
        {apps?.length === 0 ? (
          <ThemedText type="small" themeColor="textSecondary">
            Nothing installed yet.
          </ThemedText>
        ) : null}
        {apps?.map((app) => (
          <Row
            key={app.id}
            label={app.name}
            detail={appDetail(app)}
            onPress={() => {
              setSelected(app.id === selected ? null : app.id);
              setArmed(false);
            }}
          />
        ))}
        <Button title="Install from a file" disabled={!watchReady || task.busy !== null} onPress={() => void install()} />
      </Card>

      {chosen ? (
        <Card title={chosen.name}>
          <Button
            title={chosen.enabled ? 'Hide from the launcher' : 'Show in the launcher'}
            variant="secondary"
            disabled={task.busy !== null}
            onPress={() =>
              void act('Saving', async () => {
                await (await packages()).enable(chosen.id, !chosen.enabled);
              })
            }
          />
          <Button
            title={armed ? 'Tap again to remove it and what it saved' : 'Remove'}
            variant="danger"
            disabled={task.busy !== null}
            onPress={() => {
              if (!armed) {
                setArmed(true);
                return;
              }
              void act(`Removing ${chosen.name}`, async () => {
                await (await packages()).remove(chosen.id);
                setSelected(null);
              });
            }}
          />
        </Card>
      ) : null}

      {progress ?? task.busy ? (
        <ThemedText type="small" style={{ color: theme.textSecondary }}>
          {task.busy ? `${task.busy}... ${progress ?? ''}` : progress}
        </ThemedText>
      ) : null}
      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}
