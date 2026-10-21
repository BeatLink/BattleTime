import { useEffect, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { ToggleRow } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { SURVIVAL_GUIDES, type SurvivalGuide } from '@/bundles/survival-guides';
import { useWatch } from '@/hooks/use-watch';
import { guideBytes, guideIdsIn, readSurvivalList, sendSurvivalGuides } from '@/protocol/survival';
import { useTask } from '@/state/use-task';

// ============================================================================
// Choices
// ============================================================================

const KIND_TITLES: Record<SurvivalGuide['kind'], string> = {
  'first-aid': 'First aid',
  outdoors: 'Outdoors',
  disaster: 'Disasters',
};

const KINDS = Object.keys(KIND_TITLES) as SurvivalGuide['kind'][];

function sizeText(guide: SurvivalGuide): string {
  return `${(guideBytes(guide).length / 1024).toFixed(1)} kB`;
}

// ============================================================================
// The screen
// ============================================================================

// A checklist of the emergency guides, read on the watch's survival app; sending puts exactly the ticked ones on
// the watch.
export default function SurvivalScreen() {
  const { files, watchReady, capabilities } = useWatch();
  const task = useTask();
  const connected = watchReady && capabilities.files;

  const [chosen, setChosen] = useState<Set<string>>(new Set());
  // The ids of the guides on the watch, or null until they are read.
  const [onWatch, setOnWatch] = useState<string[] | null>(null);
  const [sent, setSent] = useState<string | null>(null);

  useEffect(() => {
    if (!connected) {
      return;
    }
    void task.run('Reading the guides', async () => {
      const ids = guideIdsIn(await readSurvivalList(await files()));
      setOnWatch(ids);
      setChosen(new Set(ids));
    });
    // Read once per connection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connected]);

  const toggle = (id: string, on: boolean) => {
    const next = new Set(chosen);
    if (on) {
      next.add(id);
    } else {
      next.delete(id);
    }
    setChosen(next);
    setSent(null);
  };

  const unchanged = onWatch !== null && onWatch.length === chosen.size && onWatch.every((id) => chosen.has(id));

  const send = () =>
    task.run('Sending the guides', async () => {
      const result = await sendSurvivalGuides(await files(), [...chosen]);
      setOnWatch(guideIdsIn(result.entries));
      setSent(
        result.entries.length === 0
          ? 'Took every guide off the watch.'
          : `The watch now holds ${result.entries.length} guides; ${result.sent} sent, ${result.removed} removed.`,
      );
    });

  return (
    <Screen title="Survival">
      <Card>
        <ThemedText type="small" themeColor="textSecondary">
          Tick the guides to carry on the watch&apos;s Survival app, to read without a phone or a signal. They are
          reminders of common first aid and outdoor training, not a replacement for a course. Call your emergency number
          first whenever you can.
        </ThemedText>
      </Card>

      {KINDS.map((kind) => (
        <Card key={kind} title={KIND_TITLES[kind]}>
          {SURVIVAL_GUIDES.filter((guide) => guide.kind === kind).map((guide) => (
            <div key={guide.id} class="entry">
              <ToggleRow
                label={guide.title}
                detail={sizeText(guide)}
                value={chosen.has(guide.id)}
                onValueChange={(on) => toggle(guide.id, on)}
              />
              <details class="guide-text">
                <summary>Read it</summary>
                <pre>{guide.text}</pre>
              </details>
            </div>
          ))}
        </Card>
      ))}

      <Card title="On the watch">
        <div class="actions">
          <Button
            title="Tick all"
            variant="secondary"
            disabled={chosen.size === SURVIVAL_GUIDES.length}
            onClick={() => {
              setChosen(new Set(SURVIVAL_GUIDES.map((guide) => guide.id)));
              setSent(null);
            }}
          />
          <Button
            title="Clear"
            variant="secondary"
            disabled={chosen.size === 0}
            onClick={() => {
              setChosen(new Set());
              setSent(null);
            }}
          />
        </div>
        {connected ? (
          <>
            <Button
              title={chosen.size === 0 ? 'Take every guide off' : `Send ${chosen.size} to the watch`}
              disabled={task.busy !== null || onWatch === null || unchanged}
              busy={task.busy === 'Sending the guides'}
              onClick={() => void send()}
            />
            {unchanged && !sent ? (
              <ThemedText type="small" themeColor="textSecondary">
                The watch already holds exactly these guides.
              </ThemedText>
            ) : null}
            {sent ? (
              <ThemedText type="small" themeColor="textSecondary">
                {sent}
              </ThemedText>
            ) : null}
          </>
        ) : (
          <ThemedText type="small" themeColor="textSecondary">
            Connect a watch that serves files to send the guides and see the ones it holds.
          </ThemedText>
        )}
      </Card>

      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}
