import { useEffect, useState } from 'react';
import { Image, Pressable, StyleSheet, TextInput, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ChoiceRow, ErrorNote } from '@/components/fields';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { facePreview } from '@/constants/face-previews';
import { Radius, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import type { SettingsReply, WatchSettingValues } from '@/protocol/gadgetbridge';
import { readWatchSettings, writeWatchSettings } from '@/protocol/requests';
import {
  applyPreset,
  DEFAULT_THEME,
  hexToRgb565,
  PALETTE,
  rgb565ToHex,
  THEME_PRESETS,
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

// A tile per watch face, showing its screenshot where one is bundled.
function FacePicker({ faces, value, disabled, onChange }: {
  faces: [string, string][];
  value: string | null;
  disabled: boolean;
  onChange: (path: string) => void;
}) {
  const theme = useTheme();
  return (
    <View style={styles.faces}>
      {faces.map(([path, label]) => {
        const selected = path === value;
        const preview = facePreview(path);
        return (
          <Pressable
            key={path}
            disabled={disabled}
            accessibilityRole="button"
            accessibilityLabel={label}
            accessibilityState={{ selected }}
            onPress={() => onChange(path)}
            style={[
              styles.face,
              { borderColor: selected ? theme.accent : theme.backgroundSelected, opacity: disabled ? 0.5 : 1 },
            ]}>
            {preview ? (
              <Image source={preview} style={styles.facePreview} resizeMode="contain" />
            ) : (
              <View style={[styles.facePreview, styles.faceBlank, { backgroundColor: theme.backgroundSelected }]}>
                <ThemedText type="small" themeColor="textSecondary">
                  No preview
                </ThemedText>
              </View>
            )}
            <ThemedText type="small" style={{ color: selected ? theme.accent : theme.text }}>
              {label}
            </ThemedText>
          </Pressable>
        );
      })}
    </View>
  );
}

function ColourRow({ label, colour, onChange }: { label: string; colour: number; onChange: (colour: number) => void }) {
  const theme = useTheme();
  const hex = rgb565ToHex(colour);
  const [text, setText] = useState(hex);
  const [picking, setPicking] = useState(false);

  const commit = () => {
    const parsed = hexToRgb565(text);
    if (parsed === null) {
      setText(hex);
    } else if (parsed !== colour) {
      onChange(parsed);
    }
  };

  const pick = (choice: string) => {
    const parsed = hexToRgb565(choice);
    if (parsed !== null) {
      onChange(parsed);
    }
    setPicking(false);
  };

  return (
    <View>
      <View style={styles.colour}>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={`Pick a colour for ${label}`}
          onPress={() => setPicking(!picking)}
          style={[styles.swatch, { backgroundColor: hex, borderColor: picking ? theme.accent : theme.backgroundSelected }]}
        />
        <ThemedText style={styles.colourLabel}>{label}</ThemedText>
        <TextInput
          value={text}
          onChangeText={setText}
          onBlur={commit}
          onSubmitEditing={commit}
          autoCapitalize="none"
          autoCorrect={false}
          maxLength={7}
          accessibilityLabel={`${label} colour`}
          style={[styles.hex, { color: theme.text, backgroundColor: theme.backgroundSelected }]}
        />
      </View>
      {picking ? (
        <View style={styles.palette}>
          {PALETTE.map((choice) => (
            <Pressable
              key={choice}
              accessibilityRole="button"
              accessibilityLabel={choice}
              onPress={() => pick(choice)}
              style={[styles.paletteSwatch, { backgroundColor: choice, borderColor: theme.backgroundSelected }]}
            />
          ))}
        </View>
      ) : null}
    </View>
  );
}

export default function WatchSettingsScreen() {
  const theme = useTheme();
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
            {current.launcher_style ? (
              <ChoiceRow
                label="Launcher"
                options={[
                  { value: 'Grid' as const, label: 'Grid' },
                  { value: 'List' as const, label: 'List' },
                ]}
                value={current.launcher_style}
                disabled={busy}
                onChange={(launcher_style) => change({ launcher_style })}
              />
            ) : null}
          </Card>

          <Card title="Watch face">
            <FacePicker
              faces={current.faces}
              value={current.face}
              disabled={busy}
              onChange={(face) => void change({ face })}
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
              <View style={styles.presets}>
                {THEME_PRESETS.map((preset) => (
                  <Pressable
                    key={preset.name}
                    accessibilityRole="button"
                    accessibilityLabel={`${preset.name} theme`}
                    onPress={() => setColours(applyPreset(colours, preset))}
                    style={[styles.preset, { backgroundColor: theme.backgroundSelected }]}>
                    <View style={styles.presetSwatches}>
                      {preset.accents.map((accent) => (
                        <View key={accent} style={[styles.presetSwatch, { backgroundColor: accent }]} />
                      ))}
                    </View>
                    <ThemedText type="small">{preset.name}</ThemedText>
                  </Pressable>
                ))}
              </View>
              <ThemedText type="small" themeColor="textSecondary">
                A preset sets the interface and highlight colours. Tap a swatch to pick a colour, or
                type one as #rrggbb. The watch stores fewer shades, so a colour may come back slightly
                changed. Nothing changes on the watch until you save.
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
  faces: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: Spacing.two,
  },
  face: {
    width: 100,
    alignItems: 'center',
    gap: Spacing.one,
    padding: Spacing.one,
    borderWidth: 2,
    borderRadius: Radius.small,
  },
  facePreview: {
    width: 88,
    height: 100,
  },
  faceBlank: {
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: Radius.small,
  },
  palette: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: Spacing.two,
    paddingVertical: Spacing.two,
  },
  paletteSwatch: {
    width: 32,
    height: 32,
    borderRadius: Radius.small,
    borderWidth: 1,
  },
  presets: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: Spacing.two,
  },
  preset: {
    paddingVertical: Spacing.two,
    paddingHorizontal: Spacing.three,
    borderRadius: Radius.small,
    alignItems: 'center',
    gap: Spacing.one,
  },
  presetSwatches: {
    flexDirection: 'row',
    gap: 2,
  },
  presetSwatch: {
    width: 14,
    height: 14,
    borderRadius: 3,
  },
});
