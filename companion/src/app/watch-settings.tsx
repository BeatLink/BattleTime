import { useEffect, useState } from 'react';
import { StyleSheet, TextInput, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ChoiceRow, ErrorNote } from '@/components/fields';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Radius, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import type { SettingsReply, WatchSettingValues } from '@/protocol/gadgetbridge';
import { readWatchSettings, writeWatchSettings } from '@/protocol/requests';
import {
  DEFAULT_THEME,
  hexToRgb565,
  rgb565ToHex,
  THEME_SLOTS,
  themeBytes,
  themeColours,
} from '@/protocol/theme';
import { useTask } from '@/state/use-task';

// The choices the watch's own Settings app offers, from wasp.py.
const BLANK_AFTER = [5, 10, 15, 30, 60];
const STEP_GOALS = [2000, 4000, 6000, 8000, 10000, 12000, 15000, 20000];

const LEVELS = [
  { value: 1, label: 'Low' },
  { value: 2, label: 'Mid' },
  { value: 3, label: 'High' },
];

const NOTIFY_LEVELS = [
  { value: 1, label: 'Silent' },
  { value: 2, label: 'Mid' },
  { value: 3, label: 'High' },
];

function ColourRow({ label, colour, onChange }: { label: string; colour: number; onChange: (colour: number) => void }) {
  const theme = useTheme();
  const hex = rgb565ToHex(colour);
  const [text, setText] = useState(hex);

  return (
    <View style={styles.colour}>
      <View style={[styles.swatch, { backgroundColor: hex, borderColor: theme.backgroundSelected }]} />
      <ThemedText style={styles.colourLabel}>{label}</ThemedText>
      <TextInput
        value={text}
        onChangeText={setText}
        onEndEditing={() => {
          const parsed = hexToRgb565(text);
          if (parsed === null) {
            setText(hex);
          } else {
            onChange(parsed);
          }
        }}
        autoCapitalize="none"
        autoCorrect={false}
        maxLength={7}
        accessibilityLabel={`${label} colour`}
        style={[styles.hex, { color: theme.text, backgroundColor: theme.backgroundSelected }]}
      />
    </View>
  );
}

export default function WatchSettingsScreen() {
  const { requests, watchReady } = useWatch();
  const task = useTask();
  const [current, setCurrent] = useState<SettingsReply | null>(null);
  const [colours, setColours] = useState<number[] | null>(null);

  useEffect(() => {
    if (!watchReady) {
      return;
    }
    void task.run('Reading the watch settings', async () => {
      const read = await readWatchSettings(requests);
      setCurrent(read);
      setColours(themeColours(read.theme));
    });
    // Read once per connection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watchReady]);

  // Each change goes to the watch at once, and the watch's answer is what the screen shows.
  const change = (patch: Partial<WatchSettingValues>) =>
    task.run('Saving', async () => {
      const next = await writeWatchSettings(requests, patch);
      setCurrent(next);
      setColours(themeColours(next.theme));
    });

  const busy = task.busy !== null;
  const themeChanged =
    current !== null && colours !== null && themeBytes(colours).join() !== current.theme.join();

  return (
    <Screen title="Watch settings">
      {!watchReady ? (
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            Connect to the watch to change its settings.
          </ThemedText>
        </Card>
      ) : null}

      {current ? (
        <>
          <Card title="Display">
            <ChoiceRow
              label="Brightness"
              options={LEVELS}
              value={current.brightness}
              disabled={busy}
              onChange={(brightness) => change({ brightness })}
            />
            <ChoiceRow
              label="Screen timeout"
              options={BLANK_AFTER.map((value) => ({ value, label: `${value} s` }))}
              value={current.blank_after}
              disabled={busy}
              onChange={(blank_after) => change({ blank_after })}
            />
            <ChoiceRow
              label="Clock"
              options={[
                { value: true, label: '24 hour' },
                { value: false, label: '12 hour' },
              ]}
              value={current.clock_24h}
              disabled={busy}
              onChange={(clock_24h) => change({ clock_24h })}
            />
          </Card>

          <Card title="Watch face">
            <ChoiceRow
              label="Face"
              options={current.faces.map(([path, label]) => ({ value: path, label }))}
              value={current.face ?? undefined}
              disabled={busy}
              onChange={(face) => change({ face })}
            />
          </Card>

          <Card title="Notifications and units">
            <ChoiceRow
              label="Vibration"
              detail="How hard the watch buzzes for a notification"
              options={NOTIFY_LEVELS}
              value={current.notify_level}
              disabled={busy}
              onChange={(notify_level) => change({ notify_level })}
            />
            <ChoiceRow
              label="Units"
              options={[
                { value: 'Metric' as const, label: 'Metric' },
                { value: 'Imperial' as const, label: 'Imperial' },
              ]}
              value={current.units}
              disabled={busy}
              onChange={(units) => change({ units })}
            />
            <ChoiceRow
              label="Daily step goal"
              options={STEP_GOALS.map((value) => ({ value, label: value.toLocaleString() }))}
              value={current.step_goal}
              disabled={busy}
              onChange={(step_goal) => change({ step_goal })}
            />
          </Card>

          {colours ? (
            <Card title="Theme">
              <ThemedText type="small" themeColor="textSecondary">
                Colours as #rrggbb. The watch stores fewer shades, so a colour may come back slightly
                changed.
              </ThemedText>
              {colours.map((colour, index) => (
                <ColourRow
                  // A new colour from the watch starts the box afresh.
                  key={`${index}-${colour}`}
                  label={THEME_SLOTS[index] ?? `Colour ${index + 1}`}
                  colour={colour}
                  onChange={(next) => setColours(colours.map((old, at) => (at === index ? next : old)))}
                />
              ))}
              <View style={styles.actions}>
                <Button
                  title="Save theme"
                  disabled={!themeChanged}
                  busy={busy}
                  onPress={() => void change({ theme: themeBytes(colours) })}
                />
                <Button
                  title="Reset to default"
                  variant="secondary"
                  disabled={busy}
                  onPress={() => void change({ theme: DEFAULT_THEME })}
                />
              </View>
            </Card>
          ) : null}
        </>
      ) : null}

      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}

const styles = StyleSheet.create({
  colour: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.three,
    paddingVertical: Spacing.one,
  },
  swatch: {
    width: 28,
    height: 28,
    borderRadius: Radius.small,
    borderWidth: 1,
  },
  colourLabel: {
    flex: 1,
  },
  hex: {
    width: 96,
    borderRadius: Radius.small,
    paddingHorizontal: Spacing.two,
    paddingVertical: Spacing.one,
    fontFamily: 'monospace',
  },
  actions: {
    gap: Spacing.two,
  },
});
