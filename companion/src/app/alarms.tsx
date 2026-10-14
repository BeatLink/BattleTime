import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Pressable, StyleSheet, Switch, TextInput, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Radius, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import { DAY_NAMES, MAX_ALARMS, NEW_ALARM, repeatLabel, type WatchAlarm } from '@/protocol/alarms';
import { syncBridge } from '@/services/sync';
import { changesAlarms, readAlarms, watchRecords, writeAlarms } from '@/services/watch-records';
import { useTask } from '@/state/use-task';

function TimeField({ value, max, onChange }: { value: number; max: number; onChange: (value: number) => void }) {
  const theme = useTheme();
  const [text, setText] = useState(String(value).padStart(2, '0'));
  return (
    <TextInput
      value={text}
      keyboardType="number-pad"
      maxLength={2}
      onChangeText={(next) => setText(next.replace(/[^0-9]/g, ''))}
      onEndEditing={() => {
        const number = Math.min(max, Number(text || 0));
        setText(String(number).padStart(2, '0'));
        onChange(number);
      }}
      onBlur={() => {
        const number = Math.min(max, Number(text || 0));
        setText(String(number).padStart(2, '0'));
        onChange(number);
      }}
      style={[styles.time, { color: theme.text, backgroundColor: theme.backgroundSelected }]}
    />
  );
}

function AlarmEditor({ alarm, onChange, onRemove }: {
  alarm: WatchAlarm;
  onChange: (alarm: WatchAlarm) => void;
  onRemove: () => void;
}) {
  const theme = useTheme();
  return (
    <Card>
      <View style={styles.header}>
        <View style={styles.clock}>
          <TimeField value={alarm.hour} max={23} onChange={(hour) => onChange({ ...alarm, hour })} />
          <ThemedText type="subtitle">:</ThemedText>
          <TimeField value={alarm.minute} max={59} onChange={(minute) => onChange({ ...alarm, minute })} />
        </View>
        <Switch
          value={alarm.enabled}
          onValueChange={(enabled) => onChange({ ...alarm, enabled })}
          trackColor={{ true: theme.accent, false: theme.backgroundSelected }}
          thumbColor={theme.text}
          accessibilityLabel="Alarm on"
        />
      </View>
      <View style={styles.days}>
        {DAY_NAMES.map((day, bit) => {
          const selected = (alarm.days & (1 << bit)) !== 0;
          return (
            <Pressable
              key={day}
              accessibilityRole="button"
              accessibilityState={{ selected }}
              onPress={() => onChange({ ...alarm, days: alarm.days ^ (1 << bit) })}>
              <View style={[styles.day, { backgroundColor: selected ? theme.accent : theme.backgroundSelected }]}>
                <ThemedText type="small" style={{ color: selected ? theme.onAccent : theme.text }}>
                  {day.slice(0, 2)}
                </ThemedText>
              </View>
            </Pressable>
          );
        })}
      </View>
      <View style={styles.footer}>
        <ThemedText type="small" themeColor="textSecondary">
          {repeatLabel(alarm.days)}
        </ThemedText>
        <Pressable onPress={onRemove} accessibilityRole="button">
          <ThemedText type="smallBold" style={{ color: theme.danger }}>
            Remove
          </ThemedText>
        </Pressable>
      </View>
    </Card>
  );
}

export default function AlarmsScreen() {
  const { requests, watchReady, capabilities, files, send } = useWatch();
  const task = useTask();
  const [alarms, setAlarms] = useState<WatchAlarm[] | null>(null);
  const [dirty, setDirty] = useState(false);
  const bridge = useMemo(() => syncBridge(), []);
  const records = useMemo(() => watchRecords(bridge, { files, send, requests }), [bridge, files, send, requests]);
  // The sync's copy is there while the watch is away; without it the watch must serve its files.
  const available = records.synced || (watchReady && capabilities.files);
  // A sync never replaces alarms the user is still changing.
  const editing = useRef(false);
  useEffect(() => {
    editing.current = dirty;
  }, [dirty]);

  const run = task.run;
  const load = useCallback(
    () =>
      run('Reading alarms', async () => {
        setAlarms(await readAlarms(records));
        setDirty(false);
      }),
    [records, run],
  );

  useEffect(() => {
    if (available) {
      void load();
    }
    // Read once per connection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [available]);

  useEffect(
    () => bridge?.onReport((report) => {
      if (changesAlarms(report.changed) && !editing.current) {
        void load();
      }
    }),
    [bridge, load],
  );

  const edit = (next: WatchAlarm[]) => {
    setAlarms(next);
    setDirty(true);
  };

  return (
    <Screen title="Alarms">
      {!available ? (
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            Connect to a watch that serves files to change its alarms.
          </ThemedText>
        </Card>
      ) : null}

      {alarms?.length === 0 ? (
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            No alarms are set.
          </ThemedText>
        </Card>
      ) : null}

      {alarms?.map((alarm, index) => (
        <AlarmEditor
          // The list is replaced as a whole, so its position is the alarm's identity.
          key={`${index}-${alarms.length}`}
          alarm={alarm}
          onChange={(next) => edit(alarms.map((old, at) => (at === index ? next : old)))}
          onRemove={() => edit(alarms.filter((_, at) => at !== index))}
        />
      ))}

      <ErrorNote error={task.error} onDismiss={task.clearError} />

      {alarms ? (
        <View style={styles.actions}>
          <Button
            title="Add an alarm"
            variant="secondary"
            disabled={alarms.length >= MAX_ALARMS}
            onPress={() => edit([...alarms, NEW_ALARM])}
          />
          <Button
            title="Save to watch"
            disabled={!dirty || !available}
            busy={task.busy !== null}
            onPress={() =>
              void task.run('Saving alarms', async () => {
                setAlarms(await writeAlarms(records, alarms));
                setDirty(false);
              })
            }
          />
        </View>
      ) : null}
    </Screen>
  );
}

const styles = StyleSheet.create({
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  clock: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.one,
  },
  time: {
    fontSize: 28,
    fontWeight: 600,
    width: 64,
    textAlign: 'center',
    borderRadius: Radius.small,
    paddingVertical: Spacing.one,
  },
  days: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: Spacing.two,
  },
  day: {
    width: 40,
    height: 40,
    borderRadius: 20,
    alignItems: 'center',
    justifyContent: 'center',
  },
  footer: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  actions: {
    gap: Spacing.two,
  },
});
