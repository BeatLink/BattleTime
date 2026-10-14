import { useEffect, useState } from 'react';
import { Pressable, StyleSheet, Switch, TextInput, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Radius, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import type { Alarm } from '@/protocol/gadgetbridge';
import { MAX_ALARMS, readAlarms, writeAlarms } from '@/protocol/requests';
import { useTask } from '@/state/use-task';

type EditableAlarm = Required<Alarm>;

// Days as the watch's mask numbers them, Monday first.
const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

export function repeatLabel(rep: number): string {
  if (rep === 0) return 'Once';
  if (rep === 0x7f) return 'Every day';
  if (rep === 0x1f) return 'Weekdays';
  if (rep === 0x60) return 'Weekends';
  return DAYS.filter((_, bit) => rep & (1 << bit)).join(', ');
}

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
  alarm: EditableAlarm;
  onChange: (alarm: EditableAlarm) => void;
  onRemove: () => void;
}) {
  const theme = useTheme();
  return (
    <Card>
      <View style={styles.header}>
        <View style={styles.clock}>
          <TimeField value={alarm.h} max={23} onChange={(h) => onChange({ ...alarm, h })} />
          <ThemedText type="subtitle">:</ThemedText>
          <TimeField value={alarm.m} max={59} onChange={(m) => onChange({ ...alarm, m })} />
        </View>
        <Switch
          value={alarm.on}
          onValueChange={(on) => onChange({ ...alarm, on })}
          trackColor={{ true: theme.accent, false: theme.backgroundSelected }}
          thumbColor={theme.text}
          accessibilityLabel="Alarm on"
        />
      </View>
      <View style={styles.days}>
        {DAYS.map((day, bit) => {
          const selected = (alarm.rep & (1 << bit)) !== 0;
          return (
            <Pressable
              key={day}
              accessibilityRole="button"
              accessibilityState={{ selected }}
              onPress={() => onChange({ ...alarm, rep: alarm.rep ^ (1 << bit) })}>
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
          {repeatLabel(alarm.rep)}
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
  const { requests, watchReady } = useWatch();
  const task = useTask();
  const [alarms, setAlarms] = useState<EditableAlarm[] | null>(null);
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    if (!watchReady) {
      return;
    }
    void task.run('Reading alarms', async () => {
      setAlarms(await readAlarms(requests));
      setDirty(false);
    });
    // Read once per connection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watchReady]);

  const edit = (next: EditableAlarm[]) => {
    setAlarms(next);
    setDirty(true);
  };

  return (
    <Screen title="Alarms">
      {!watchReady ? (
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            Connect to the watch to change its alarms.
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
            onPress={() => edit([...alarms, { h: 7, m: 0, on: true, rep: 0x1f }])}
          />
          <Button
            title="Save to watch"
            disabled={!dirty || !watchReady}
            busy={task.busy !== null}
            onPress={() =>
              void task.run('Saving alarms', async () => {
                setAlarms(await writeAlarms(requests, alarms));
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
