import { useCallback, useEffect, useState } from 'react';
import { Pressable, StyleSheet, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import { readWatchSettings } from '@/protocol/requests';
import { readStepHistory, type StepDay } from '@/protocol/steps';
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
  const theme = useTheme();
  const top = Math.max(1, goal ?? 0, ...bars.map((bar) => bar.value));
  const picked = selected !== null ? bars[selected] : null;

  return (
    <View style={styles.chart}>
      <ThemedText type="small" themeColor="textSecondary" style={styles.readout}>
        {picked ? `${picked.detail}: ${steps(picked.value)} steps` : 'Tap a bar to read it'}
      </ThemedText>
      <View style={[styles.plot, { height: CHART_HEIGHT, borderBottomColor: theme.backgroundSelected }]}>
        {goal ? (
          <View
            pointerEvents="none"
            style={[styles.goal, { bottom: (goal / top) * CHART_HEIGHT, borderColor: theme.textSecondary }]}
          />
        ) : null}
        {bars.map((bar, index) => (
          <Pressable
            key={bar.key}
            style={styles.slot}
            accessibilityLabel={`${bar.detail}, ${steps(bar.value)} steps`}
            onPress={() => onSelect(index)}>
            <View
              style={[
                styles.bar,
                {
                  height: Math.max(bar.value > 0 ? 2 : 0, (bar.value / top) * CHART_HEIGHT),
                  backgroundColor: theme.accent,
                  opacity: selected === null || selected === index ? 1 : 0.45,
                },
              ]}
            />
          </Pressable>
        ))}
      </View>
      <View style={styles.labels}>
        {bars.map((bar) => (
          <ThemedText key={bar.key} type="small" themeColor="textSecondary" style={styles.label} numberOfLines={1}>
            {bar.label}
          </ThemedText>
        ))}
      </View>
    </View>
  );
}

export default function StepsScreen() {
  const theme = useTheme();
  const { requests, watchReady } = useWatch();
  const task = useTask();
  const [history, setHistory] = useState<StepDay[]>([]);
  const [goal, setGoal] = useState<number | undefined>(undefined);
  const [day, setDay] = useState(0);
  const [hour, setHour] = useState<number | null>(null);

  const load = useCallback(
    () =>
      task.run('Reading the step log', async () => {
        const settings = await readWatchSettings(requests);
        setGoal(settings.step_goal || undefined);
        setHistory(await readStepHistory(requests, DAYS));
      }),
    [requests, task],
  );

  useEffect(() => {
    if (watchReady) {
      void load();
    }
    // Read once per connection, not on every render of the task.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watchReady]);

  const today = history[0];
  // Oldest on the left, as a week reads.
  const week = [...history].reverse();
  const shown = history[day];

  return (
    <Screen title="Steps">
      {!watchReady ? (
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            Connect to the watch to read its step log.
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
          {shown.hours.length === 0 ? (
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
        <ThemedText type="small" style={{ color: theme.textSecondary }}>
          The watch logs steps every 6 minutes and keeps every day on its flash.
        </ThemedText>
      )}
      <Button
        title="Read again"
        variant="secondary"
        busy={task.busy !== null}
        disabled={!watchReady}
        onPress={() => void load()}
      />
    </Screen>
  );
}

const styles = StyleSheet.create({
  chart: {
    gap: Spacing.two,
  },
  readout: {
    minHeight: 20,
  },
  plot: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    borderBottomWidth: 1,
  },
  slot: {
    flex: 1,
    height: '100%',
    justifyContent: 'flex-end',
    // A 2px gap between neighbouring bars.
    paddingHorizontal: 1,
  },
  bar: {
    borderTopLeftRadius: 4,
    borderTopRightRadius: 4,
  },
  goal: {
    position: 'absolute',
    left: 0,
    right: 0,
    borderTopWidth: 1,
    borderStyle: 'dashed',
  },
  labels: {
    flexDirection: 'row',
  },
  label: {
    flex: 1,
    textAlign: 'center',
    fontSize: 11,
  },
});
