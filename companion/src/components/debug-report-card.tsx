import { useState } from 'preact/hooks';

import { useWatch } from '@/hooks/use-watch';
import { crashSummary, saveDebugReport } from '@/protocol/debug-report';
import { saveFile } from '@/services/device-files';
import { useTask } from '@/state/use-task';

import { Button } from './button';
import { Card } from './card';
import { ThemedText } from './themed-text';

// Saves a debug report of the watch's last crash, which also lets the watch leave its crash screen.
export function DebugReportCard() {
  const { dfuLink, files, packages, capabilities } = useWatch();
  const task = useTask();
  const [summary, setSummary] = useState<string | null>(null);

  const save = () =>
    task.run('Saving', async () => {
      const link = dfuLink();
      if (!link) {
        throw new Error('Connect to a watch first.');
      }
      const report = await saveDebugReport(
        link,
        {
          apps: capabilities.packages ? async () => (await packages()).apps() : undefined,
          freeStorageBytes: capabilities.files ? async () => (await (await files()).storage()).free : undefined,
        },
        async (fileName, json) => saveFile(fileName, json, 'application/json'),
      );
      setSummary(crashSummary(report.lastRestart));
    });

  return (
    <Card title="Debug report">
      <ThemedText type="small" themeColor="textSecondary">
        Saves why the watch last restarted, with its firmware, counters, apps and free storage. After a
        crash the watch waits on its crash screen until this is saved.
      </ThemedText>
      {summary ? <ThemedText type="small">{summary}</ThemedText> : null}
      {task.error ? (
        <ThemedText type="small" themeColor="danger">
          {task.error}
        </ThemedText>
      ) : null}
      <Button title="Save debug report" variant="secondary" busy={task.busy !== null} onClick={() => void save()} />
    </Card>
  );
}
