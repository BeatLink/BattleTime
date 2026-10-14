import { useEffect, useState } from 'preact/hooks';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { ChoiceRow, ErrorNote } from '@/components/fields';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { useWatch } from '@/hooks/use-watch';
import {
  THEME_COLOR_KEYS,
  type SettingsMessage,
  type SettingsReply,
  type ThemeColors,
} from '@/protocol/gadgetbridge';
import { readWatchSettings, writeWatchSettings } from '@/protocol/requests';
import { changedColors, normaliseHex, PALETTE, THEME_COLOR_LABELS } from '@/protocol/theme';
import { useTask } from '@/state/use-task';

// The choices offered here, inside the limits the watch accepts.
const BLANK_AFTER = [5, 10, 15, 30, 60, 120, 300];
const STEP_GOALS = [2000, 4000, 6000, 8000, 10000, 12000, 15000, 20000];

const LEVELS = [
  { value: 1, label: 'Low' },
  { value: 2, label: 'Mid' },
  { value: 3, label: 'High' },
];

const CHIMES = [
  { value: 0 as const, label: 'Off' },
  { value: 1 as const, label: 'Hourly' },
  { value: 2 as const, label: 'Half-hourly' },
];

const ON_OFF = [
  { value: true, label: 'On' },
  { value: false, label: 'Off' },
];

const titleCase = (name: string) => name.charAt(0).toUpperCase() + name.slice(1);

function ColourRow({ label, colour, onChange }: { label: string; colour: string; onChange: (colour: string) => void }) {
  const [text, setText] = useState(colour);
  const [picking, setPicking] = useState(false);

  const commit = () => {
    const parsed = normaliseHex(text);
    if (parsed === null) {
      setText(colour);
    } else if (parsed !== colour) {
      onChange(parsed);
    }
  };

  const pick = (choice: string) => {
    onChange(choice);
    setPicking(false);
  };

  return (
    <div>
      <div class="line" style={{ padding: '4px 0' }}>
        <button
          type="button"
          class={picking ? 'swatch picking' : 'swatch'}
          aria-label={`Pick a colour for ${label}`}
          aria-expanded={picking}
          onClick={() => setPicking(!picking)}
          style={{ backgroundColor: colour }}
        />
        <ThemedText class="grow">{label}</ThemedText>
        <input
          class="input hex-field"
          value={text}
          onInput={(event) => setText(event.currentTarget.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === 'Enter') commit();
          }}
          autoCapitalize="none"
          autoComplete="off"
          spellcheck={false}
          maxLength={7}
          aria-label={`${label} colour`}
        />
      </div>
      {picking ? (
        <div class="actions" style={{ padding: '8px 0' }}>
          {PALETTE.map((choice) => (
            <button
              type="button"
              key={choice}
              class="swatch large"
              aria-label={choice}
              onClick={() => pick(choice)}
              style={{ backgroundColor: choice }}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

export default function WatchSettingsScreen() {
  const { requests, watchReady } = useWatch();
  const task = useTask();
  const [current, setCurrent] = useState<SettingsReply | null>(null);
  const [colours, setColours] = useState<ThemeColors | null>(null);

  useEffect(() => {
    if (!watchReady) {
      return;
    }
    void task.run('Reading the watch settings', async () => {
      const read = await readWatchSettings(requests);
      setCurrent(read);
      setColours(read.theme_colors);
    });
    // Read once per connection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watchReady]);

  // Each change goes to the watch at once, and the watch's answer is what the screen shows.
  const change = (patch: Omit<SettingsMessage, 't'>) =>
    task.run('Saving', async () => {
      const next = await writeWatchSettings(requests, patch);
      setCurrent(next);
      setColours(next.theme_colors);
    });

  const busy = task.busy !== null;
  const edited = current && colours ? changedColors(current.theme_colors, colours) : {};
  const themeChanged = Object.keys(edited).length > 0;

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
            {current.blank_after !== undefined ? (
              <ChoiceRow
                label="Screen timeout"
                options={BLANK_AFTER.map((value) => ({ value, label: value < 60 ? `${value} s` : `${value / 60} min` }))}
                value={current.blank_after}
                disabled={busy}
                onChange={(blank_after) => change({ blank_after })}
              />
            ) : null}
            {current.always_on !== undefined ? (
              <ChoiceRow
                label="Always on"
                detail="Keep a dim clock on the screen when it would otherwise turn off"
                options={ON_OFF}
                value={current.always_on}
                disabled={busy}
                onChange={(always_on) => change({ always_on })}
              />
            ) : null}
            {current.clock_24h !== undefined ? (
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
            ) : null}
          </Card>

          {current.do_not_disturb !== undefined || current.chimes !== undefined ? (
            <Card title="Quiet and chimes">
              {current.do_not_disturb !== undefined ? (
                <ChoiceRow
                  label="Do not disturb"
                  detail="Notifications arrive without a buzz"
                  options={ON_OFF}
                  value={current.do_not_disturb}
                  disabled={busy}
                  onChange={(do_not_disturb) => change({ do_not_disturb })}
                />
              ) : null}
              {current.chimes !== undefined ? (
                <ChoiceRow
                  label="Chimes"
                  options={CHIMES}
                  value={current.chimes}
                  disabled={busy}
                  onChange={(chimes) => change({ chimes })}
                />
              ) : null}
            </Card>
          ) : null}

          {current.units !== undefined || current.step_goal !== undefined ? (
            <Card title="Units and goals">
              {current.units !== undefined ? (
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
              ) : null}
              {current.step_goal !== undefined ? (
                <ChoiceRow
                  label="Daily step goal"
                  options={STEP_GOALS.map((value) => ({ value, label: value.toLocaleString() }))}
                  value={current.step_goal}
                  disabled={busy}
                  onChange={(step_goal) => change({ step_goal })}
                />
              ) : null}
            </Card>
          ) : null}

          {current.gadgetbridge_name !== undefined ? (
            <Card title="Bluetooth">
              <ChoiceRow
                label="Show as InfiniTime"
                detail="Advertise the name InfiniTime, so Gadgetbridge offers its InfiniTime support for this watch"
                options={ON_OFF}
                value={current.gadgetbridge_name}
                disabled={busy}
                onChange={(gadgetbridge_name) => change({ gadgetbridge_name })}
              />
            </Card>
          ) : null}

          {colours ? (
            <Card title="Theme">
              <div class="actions">
                {current.themes.map((name) => (
                  <button
                    type="button"
                    key={name}
                    class={name === current.theme ? 'preset selected text-small' : 'preset text-small'}
                    aria-label={`${name} theme`}
                    aria-pressed={name === current.theme}
                    disabled={busy}
                    onClick={() => void change({ theme: name })}>
                    {titleCase(name)}
                  </button>
                ))}
              </div>
              <ThemedText type="small" themeColor="textSecondary">
                A preset changes the watch at once. Tap a swatch to pick a colour, or type one as
                #rrggbb, then save. The watch stores fewer shades, so a colour may come back slightly
                changed.
              </ThemedText>
              {THEME_COLOR_KEYS.map((key) => (
                <ColourRow
                  // A new colour from the watch starts the box afresh.
                  key={`${key}-${colours[key]}`}
                  label={THEME_COLOR_LABELS[key]}
                  colour={colours[key]}
                  onChange={(next) => setColours({ ...colours, [key]: next })}
                />
              ))}
              <Button
                title="Save colours"
                disabled={!themeChanged}
                busy={busy}
                onClick={() => void change({ theme_colors: edited })}
              />
            </Card>
          ) : null}
        </>
      ) : null}

      <ErrorNote error={task.error} onDismiss={task.clearError} />
    </Screen>
  );
}
