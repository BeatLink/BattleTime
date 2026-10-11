import * as DocumentPicker from 'expo-document-picker';
import { useCallback, useEffect, useState } from 'react';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import { useTask } from '@/state/use-task';

// The app id is the image's file name, as the NeoTime compiler names its output.
function appIdFrom(fileName: string): string {
  return fileName.replace(/\.ntapp$/i, '').replace(/[^a-z0-9_-]/gi, '-').toLowerCase();
}

// Lists the apps on the watch, installs a compiled .ntapp image and removes an app with its saved state.
export default function AppsScreen() {
  const theme = useTheme();
  const { files, watchReady } = useWatch();
  const task = useTask();
  const [apps, setApps] = useState<string[] | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [armed, setArmed] = useState<string | null>(null);

  const refresh = useCallback(
    () =>
      task.run('Listing apps', async () => {
        const entries = await (await files()).list('/apps');
        setApps(entries.filter((entry) => entry.directory).map((entry) => entry.name));
      }),
    [files, task],
  );

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
      const id = appIdFrom(asset.name);
      await (await files()).installApp(id, image, (step) =>
        setProgress(`${id}: ${Math.round((step.sent / step.total) * 100)}%`),
      );
      setProgress(null);
      await refresh();
    });

  const remove = (id: string) => {
    if (armed !== id) {
      setArmed(id);
      return;
    }
    void task.run(`Removing ${id}`, async () => {
      await (await files()).removeApp(id);
      setArmed(null);
      await refresh();
    });
  };

  return (
    <Screen title="Apps">
      <Card>
        <ThemedText type="small" themeColor="textSecondary">
          Apps are compiled by the NeoTime compiler into .ntapp images. Installing one sends it to
          the watch, which checks it before adding it. Removing an app also deletes what it saved.
        </ThemedText>
      </Card>

      <Card title="On the watch">
        {apps?.length === 0 ? (
          <ThemedText type="small" themeColor="textSecondary">
            No apps installed.
          </ThemedText>
        ) : null}
        {apps?.map((id) => (
          <Row
            key={id}
            label={id}
            detail={armed === id ? 'Tap again to remove it and its saved state' : 'Tap to remove'}
            onPress={() => remove(id)}
            right={
              armed === id ? (
                <ThemedText type="smallBold" style={{ color: theme.danger }}>
                  Remove
                </ThemedText>
              ) : null
            }
          />
        ))}
        <Button title="Install an app" disabled={!watchReady || task.busy !== null} onPress={() => void install()} />
      </Card>

      {progress ?? task.busy ? (
        <ThemedText type="small" themeColor="textSecondary">
          {progress ?? `${task.busy}...`}
        </ThemedText>
      ) : null}
      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}
