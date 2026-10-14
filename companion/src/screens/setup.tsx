import { useMemo, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import { useRouter } from '@/navigation';
import { runSetup, setupItems, type SetupItem } from '@/protocol/setup';
import { useTask } from '@/state/use-task';

const KIND_LABEL = { library: 'Shared icons', face: 'Watch face', app: 'App' } as const;

// Guides a new watch from its help screen to a working one: a face, the launcher and the apps.
export default function SetupScreen() {
  const router = useRouter();
  const { files, packages, watchReady } = useWatch();
  const task = useTask();
  const items = useMemo(() => setupItems(), []);
  const [chosen, setChosen] = useState<Set<string>>(() => new Set(items.map((item) => item.file)));
  const [step, setStep] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  const toggle = (item: SetupItem, on: boolean) => {
    const next = new Set(chosen);
    if (on) next.add(item.file);
    else next.delete(item.file);
    setChosen(next);
  };

  const install = () =>
    task.run('Installing', async () => {
      const picked = items.filter((item) => item.required || chosen.has(item.file));
      await runSetup(await files(), await packages(), picked, (progress) =>
        setStep(
          `${progress.index + 1} of ${progress.count}: ${progress.item.header.name}, ${Math.round(
            (progress.sent / Math.max(1, progress.total)) * 100,
          )}%`,
        ),
      );
      setStep(null);
      setDone(true);
    });

  return (
    <Screen title="Set up the watch">
      <Card>
        <ThemedText type="small" themeColor="textSecondary">
          NeoTime starts with no apps, only a help screen. This sends the shared icons, a watch face,
          the launcher and the apps you choose. It takes a minute or two over Bluetooth; keep the
          watch close.
        </ThemedText>
      </Card>

      <Card title="What to install">
        {items.map((item) => (
          <ToggleRow
            key={item.file}
            label={item.header.name}
            detail={`${KIND_LABEL[item.header.kind]} · ${(item.image.length / 1024).toFixed(1)} kB${item.required ? ' · needed' : ''}`}
            value={item.required || chosen.has(item.file)}
            disabled={item.required || task.busy !== null}
            onValueChange={(on) => toggle(item, on)}
          />
        ))}
        <Button title="Install" busy={task.busy !== null} disabled={!watchReady || done} onClick={() => void install()} />
      </Card>

      {step ? (
        <ThemedText type="small" themeColor="textSecondary">
          {step}
        </ThemedText>
      ) : null}

      {done ? (
        <Card>
          <ThemedText type="smallBold">The watch is set up.</ThemedText>
          <ThemedText type="small" themeColor="textSecondary">
            It shows the face now. Swipe up on the face for the launcher.
          </ThemedText>
          <Button title="Done" onClick={() => router.back()} />
        </Card>
      ) : null}

      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}
