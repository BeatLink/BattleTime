import { useCallback, useEffect, useMemo, useRef, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ErrorNote } from '@/components/fields';
import { Switch } from '@/components/row';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { theme } from '@/constants/theme';
import { useWatch } from '@/hooks/use-watch';
import { DAY_NAMES, MAX_ALARMS, NEW_ALARM, repeatLabel, type WatchAlarm } from '@/protocol/alarms';
import { syncBridge } from '@/services/sync';
import { changesAlarms, readAlarms, watchRecords, writeAlarms } from '@/services/watch-records';
import { useTask } from '@/state/use-task';

// A two-digit box for the hour or the minute, which settles on a number in range when it loses focus.
function TimeField({ value, max, label, onChange }: { value: number; max: number; label: string; onChange: (value: number) => void }) {
  const [text, setText] = useState(String(value).padStart(2, '0'));
  const settle = () => {
    const number = Math.min(max, Number(text || 0));
    setText(String(number).padStart(2, '0'));
    onChange(number);
  };
  return (
    <input
      class="input time-field"
      aria-label={label}
      value={text}
      inputMode="numeric"
      maxLength={2}
      onInput={(event) => setText(event.currentTarget.value.replace(/[^0-9]/g, ''))}
      onBlur={settle}
      onKeyDown={(event) => {
        if (event.key === 'Enter') settle();
      }}
    />
  );
}

function AlarmEditor({ alarm, onChange, onRemove }: {
  alarm: WatchAlarm;
  onChange: (alarm: WatchAlarm) => void;
  onRemove: () => void;
}) {
  return (
    <Card>
      <div class="split">
        <div class="line" style={{ gap: '4px' }}>
          <TimeField value={alarm.hour} max={23} label="Hour" onChange={(hour) => onChange({ ...alarm, hour })} />
          <ThemedText type="subtitle">:</ThemedText>
          <TimeField value={alarm.minute} max={59} label="Minute" onChange={(minute) => onChange({ ...alarm, minute })} />
        </div>
        <Switch value={alarm.enabled} onValueChange={(enabled) => onChange({ ...alarm, enabled })} label="Alarm on" />
      </div>
      <div class="actions">
        {DAY_NAMES.map((day, bit) => {
          const selected = (alarm.days & (1 << bit)) !== 0;
          return (
            <button
              type="button"
              key={day}
              class={selected ? 'day-button selected' : 'day-button'}
              aria-label={day}
              aria-pressed={selected}
              onClick={() => onChange({ ...alarm, days: alarm.days ^ (1 << bit) })}>
              {day.slice(0, 2)}
            </button>
          );
        })}
      </div>
      <div class="split">
        <ThemedText type="small" themeColor="textSecondary">
          {repeatLabel(alarm.days)}
        </ThemedText>
        <button type="button" class="text-button text-small-bold" style={{ color: theme.danger }} onClick={onRemove}>
          Remove
        </button>
      </div>
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
        <div class="stack">
          <Button
            title="Add an alarm"
            variant="secondary"
            disabled={alarms.length >= MAX_ALARMS}
            onClick={() => edit([...alarms, NEW_ALARM])}
          />
          <Button
            title="Save to watch"
            disabled={!dirty || !available}
            busy={task.busy !== null}
            onClick={() =>
              void task.run('Saving alarms', async () => {
                setAlarms(await writeAlarms(records, alarms));
                setDirty(false);
              })
            }
          />
        </div>
      ) : null}
    </Screen>
  );
}
