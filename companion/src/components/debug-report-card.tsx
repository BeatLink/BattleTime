import { useState } from 'react';
import { Platform, Share } from 'react-native';

import { Button } from './button';
import { Card } from './card';
import { ThemedText } from './themed-text';

import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import { crashSummary, saveDebugReport } from '@/protocol/debug-report';
import { useTask } from '@/state/use-task';

// Downloads the report in a browser or the desktop app, and offers it to the share sheet on a phone.
async function keepReport(fileName: string, json: string): Promise<void> {
  if (Platform.OS === 'web') {
    const url = URL.createObjectURL(new Blob([json], { type: 'application/json' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = fileName;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    return;
  }
  await Share.share({ title: fileName, message: json });
}

// Saves a debug report of the watch's last crash, which also lets the watch leave its crash screen.
export function DebugReportCard() {
  const theme = useTheme();
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
        keepReport,
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
        <ThemedText type="small" style={{ color: theme.danger }}>
          {task.error}
        </ThemedText>
      ) : null}
      <Button title="Save debug report" variant="secondary" busy={task.busy !== null} onPress={() => void save()} />
    </Card>
  );
}
