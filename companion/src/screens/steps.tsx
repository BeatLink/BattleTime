import { useCallback, useEffect, useMemo, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { theme } from '@/constants/theme';
import { useWatch } from '@/hooks/use-watch';
import { readStepHistory, type StepDay } from '@/protocol/steps';
import { syncBridge } from '@/services/sync';
import { changesSteps, watchRecords } from '@/services/watch-records';
import { useTask } from '@/state/use-task';

const DAYS = 7;
const CHART_HEIGHT = 140;

const dayName = (date: Date, index: number) =>
  index === 0 ? 'Today' : date.toLocaleDateString(undefined, { weekday: 'short' });

const steps = (count: number) => count.toLocaleString();

type Bar = { key: string; label: string; value: number; detail: string };

// Vertical bars on one baseline; a tapped bar shows its value above the chart.
function BarChart({ bars, goal, selected, onSelect }: {
  bars: Bar[];
  goal?: number;
  selected: number | null;
  onSelect: (index: number) => void;
}) {
  const top = Math.max(1, goal ?? 0, ...bars.map((bar) => bar.value));
  const picked = selected !== null ? bars[selected] : null;

  return (
    <div class="stack">
      <ThemedText type="small" themeColor="textSecondary" style={{ minHeight: '20px' }}>
        {picked ? `${picked.detail}: ${steps(picked.value)} steps` : 'Tap a bar to read it'}
      </ThemedText>
      <div class="chart-plot" style={{ height: `${CHART_HEIGHT}px` }}>
        {goal ? <div class="chart-goal" style={{ bottom: `${(goal / top) * CHART_HEIGHT}px` }} /> : null}
        {bars.map((bar, index) => (
          <button
            type="button"
            key={bar.key}
            class="chart-slot"
            aria-label={`${bar.detail}, ${steps(bar.value)} steps`}
            onClick={() => onSelect(index)}>
            <div
              class="chart-bar"
              style={{
                height: `${Math.max(bar.value > 0 ? 2 : 0, (bar.value / top) * CHART_HEIGHT)}px`,
                opacity: selected === null || selected === index ? 1 : 0.45,
              }}
            />
          </button>
        ))}
      </div>
      <div class="chart-labels">
        {bars.map((bar) => (
          <span key={bar.key} class="chart-label" style={{ color: theme.textSecondary }}>
            {bar.label}
          </span>
        ))}
      </div>
    </div>
  );
}

export default function StepsScreen() {
  const { requests, watchReady, capabilities, files, send } = useWatch();
  const task = useTask();
  const bridge = useMemo(() => syncBridge(), []);
  const records = useMemo(() => watchRecords(bridge, { files, send, requests }), [bridge, files, send, requests]);
  // The sync's copy is there while the watch is away; without it the watch must serve its files.
  const available = records.synced || (watchReady && capabilities.files);
  const [history, setHistory] = useState<StepDay[]>([]);
  const [goal, setGoal] = useState<number | undefined>(undefined);
  const [day, setDay] = useState(0);
  const [hour, setHour] = useState<number | null>(null);

  const run = task.run;
  const load = useCallback(
    () =>
      run('Reading the step log', async () => {
        setGoal((await records.readStepGoal()) ?? undefined);
        setHistory(await readStepHistory(records.readStepDay, DAYS));
      }),
    [records, run],
  );

  useEffect(() => {
    if (available) {
      void load();
    }
    // Read once per connection, not on every render of the task.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [available]);

  // A sync that brings new steps reads them again.
  useEffect(
    () => bridge?.onReport((report) => {
      if (changesSteps(report.changed)) {
        void load();
      }
    }),
    [bridge, load],
  );

  const today = history[0];
  // Oldest on the left, as a week reads.
  const week = [...history].reverse();
  const shown = history[day];

  return (
    <Screen title="Steps">
      {!available ? (
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            Connect to a watch that serves files to read its step log.
          </ThemedText>
        </Card>
      ) : null}

      {today ? (
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            Today
          </ThemedText>
          <ThemedText type="title">{steps(today.total)}</ThemedText>
          {goal ? (
            <ThemedText type="small" themeColor="textSecondary">
              {today.total >= goal ? 'Goal reached' : `${steps(goal - today.total)} to go`} of a {steps(goal)} step goal
            </ThemedText>
          ) : null}
        </Card>
      ) : null}

      {history.length > 0 ? (
        <Card title={`Last ${DAYS} days`}>
          <BarChart
            bars={week.map((entry, index) => ({
              key: entry.date.toISOString(),
              label: dayName(entry.date, history.length - 1 - index),
              value: entry.total,
              detail: entry.date.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'short' }),
            }))}
            goal={goal}
            selected={history.length - 1 - day}
            onSelect={(index) => {
              setDay(history.length - 1 - index);
              setHour(null);
            }}
          />
        </Card>
      ) : null}

      {shown ? (
        <Card title={`${dayName(shown.date, day)} by hour`}>
          {shown.slots === null ? (
            <ThemedText type="small" themeColor="textSecondary">
              The watch has no step log for this day.
            </ThemedText>
          ) : (
            <BarChart
              bars={shown.hours.map((count, index) => ({
                key: String(index),
                label: index % 6 === 0 ? String(index).padStart(2, '0') : '',
                value: count,
                detail: `${String(index).padStart(2, '0')}:00 to ${String(index + 1).padStart(2, '0')}:00`,
              }))}
              selected={hour}
              onSelect={setHour}
            />
          )}
        </Card>
      ) : null}

      <ErrorNote error={task.error} onDismiss={task.clearError} />
      {task.error ? null : (
        <ThemedText type="small" themeColor="textSecondary">
          The watch logs steps every 6 minutes and keeps every day on its flash.
        </ThemedText>
      )}
      <Button
        title="Read again"
        variant="secondary"
        busy={task.busy !== null}
        disabled={!available}
        onClick={() => void load()}
      />
    </Screen>
  );
}
