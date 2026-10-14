import { useCallback, useEffect, useMemo, useState } from 'react';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { PluginPage } from '@/components/plugin-page';
import { Row } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import { installImage } from '@/protocol/packages';
import {
  appendLog,
  describePermissions,
  installPluginWatchApps,
  pluginBridge,
  Plugins,
  pluginStateText,
  type PluginLogLine,
  type PluginStatus,
} from '@/services/plugins';
import { useTask } from '@/state/use-task';

// Installs, turns on and off, and removes plugins, installs the watch apps they carry, and shows their settings.
export default function PluginsScreen() {
  const bridge = useMemo(() => pluginBridge(), []);
  const plugins = useMemo(() => (bridge ? new Plugins(bridge) : null), [bridge]);
  const { files, packages, watchReady } = useWatch();
  const task = useTask();
  const [list, setList] = useState<PluginStatus[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [armed, setArmed] = useState(false);
  const [log, setLog] = useState<PluginLogLine[]>([]);
  const [progress, setProgress] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (plugins) {
      setList(await plugins.list());
    }
  }, [plugins]);

  useEffect(() => {
    void task.run('Listing plugins', refresh);
    const stop = bridge?.onLog((line) => setLog((lines) => appendLog(lines, line)));
    return () => stop?.();
    // List once when the screen opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!bridge || !plugins) {
    return (
      <Screen title="Plugins">
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            Plugins run in the desktop app for now. Each one is the phone side of a watch app: it carries the
            app, keeps its settings and answers it while the watch is connected.
          </ThemedText>
        </Card>
      </Screen>
    );
  }

  const installWatchApps = (id: string) =>
    installPluginWatchApps(plugins, id, async (image) => {
      const header = await installImage(await files(), await packages(), image);
      setProgress(`Installed ${header.name} on the watch.`);
    });

  const install = () =>
    task.run('Installing a plugin', async () => {
      const installed = await plugins.installPick();
      if (!installed) {
        return;
      }
      setSelected(installed.manifest.id);
      setProgress(`Installed ${installed.manifest.name} ${installed.manifest.version}.`);
      if (watchReady && installed.manifest.watch.length > 0) {
        await installWatchApps(installed.manifest.id);
      }
      await refresh();
    });

  const act = (label: string, action: () => Promise<void>) =>
    task.run(label, async () => {
      try {
        await action();
      } finally {
        setArmed(false);
        await refresh();
      }
    });

  const chosen = list.find((plugin) => plugin.manifest.id === selected) ?? null;
  const chosenLog = log.filter((line) => line.id === chosen?.manifest.id);

  return (
    <Screen title="Plugins">
      <Card>
        <ThemedText type="small" themeColor="textSecondary">
          A plugin is the phone side of a watch app, in one .ntplugin file. Installing it puts its app on the
          watch and starts its logic here, which answers the app and keeps its settings. It can only send the
          watch the messages and reach the websites it lists.
        </ThemedText>
        <Button title="Install a plugin" disabled={task.busy !== null} onPress={() => void install()} />
      </Card>

      <Card title="Installed">
        {list.length === 0 ? (
          <ThemedText type="small" themeColor="textSecondary">
            No plugins yet.
          </ThemedText>
        ) : null}
        {list.map((plugin) => (
          <Row
            key={plugin.manifest.id}
            label={`${plugin.manifest.name} ${plugin.manifest.version}`}
            detail={pluginStateText(plugin)}
            onPress={() => {
              setSelected(plugin.manifest.id === selected ? null : plugin.manifest.id);
              setArmed(false);
            }}
          />
        ))}
      </Card>

      {chosen ? (
        <Card title={chosen.manifest.name}>
          {chosen.manifest.description ? (
            <ThemedText type="small" themeColor="textSecondary">
              {chosen.manifest.description}
            </ThemedText>
          ) : null}
          {describePermissions(chosen.manifest).map((permission) => (
            <ThemedText key={permission} type="small">
              • {permission}
            </ThemedText>
          ))}
          {chosen.manifest.watch.length > 0 ? (
            <Button
              title="Install its app on the watch"
              variant="secondary"
              disabled={!watchReady || task.busy !== null}
              onPress={() => void act('Installing on the watch', async () => void (await installWatchApps(chosen.manifest.id)))}
            />
          ) : null}
          <Button
            title={chosen.enabled ? 'Turn off' : 'Turn on'}
            variant="secondary"
            disabled={task.busy !== null}
            onPress={() => void act('Saving', () => plugins.setEnabled(chosen.manifest.id, !chosen.enabled))}
          />
          <Button
            title={armed ? 'Tap again to remove it and its settings' : 'Remove'}
            variant="danger"
            disabled={task.busy !== null}
            onPress={() => {
              if (!armed) {
                setArmed(true);
                return;
              }
              void act(`Removing ${chosen.manifest.name}`, async () => {
                await plugins.remove(chosen.manifest.id);
                setSelected(null);
              });
            }}
          />
          {chosen.manifest.ui && chosen.running ? <PluginPage id={chosen.manifest.id} bridge={bridge} /> : null}
          {chosenLog.map((line, index) => (
            <ThemedText key={index} type="small" themeColor={line.line.startsWith('error') ? 'danger' : 'textSecondary'}>
              {line.line}
            </ThemedText>
          ))}
        </Card>
      ) : null}

      {progress ?? task.busy ? (
        <ThemedText type="small" themeColor="textSecondary">
          {task.busy ? `${task.busy}...` : progress}
        </ThemedText>
      ) : null}
      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}
