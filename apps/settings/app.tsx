// Settings, merged from InfiniTime's Settings and wasp-os's settings app.
// From InfiniTime: one list of what can be changed, each opening its own screen of choices, the
// display timeout, the 12 or 24 hour clock, the step goal and the hourly chimes.
// From wasp-os: brightness in three steps, units, and do not disturb.
// Also: what touch wakes the screen, whether alarms ring in do not disturb, Bluetooth until the next
// restart, and whether the watch takes firmware updates.
// The theme is one of the watch's presets; the companion app can also set colours of the user's own.
// Every change goes to the watch's settings with `set_setting`, so the companion app sees it too.

import { computed } from "@preact/signals";
import {
  brightness,
  define_app,
  type Color,
  go,
  is_bluetooth_enabled,
  is_do_not_disturb,
  range,
  set_bluetooth,
  set_do_not_disturb,
  set_setting,
  settings,
} from "neotime";
import { icons } from "neotime/icons";
import { Button, Column, Grid, Icon, Label, Row, Screen, Scroll, Stepper, Switch } from "neotime/widgets";

// ============================================================================
// Choices
// ============================================================================

const BRIGHTNESS_NAMES = ["Low", "Medium", "High"];

const TIMEOUTS_IN_SECONDS = [5, 10, 15, 30, 60, 120];
const TIMEOUT_COUNT = 6;

const CHIME_NAMES = ["Off", "Every hour", "Every half hour"];

const WAKE_NAMES = ["Button only", "Tap", "Double tap"];

/** The watch's preset themes in its own order, each shown by its background and accent colours. */
const THEME_NAMES = ["Dark", "Light", "Ocean", "Forest", "Sunset", "Contrast"];
const THEME_BACKGROUNDS: Color[] = ["#000000", "#ffffff", "#000a1e", "#08140a", "#180810", "#000000"];
const THEME_ACCENTS: Color[] = ["#00aaa0", "#008096", "#28a0ff", "#78c850", "#ff783c", "#ffdc00"];
const THEME_COUNT = 6;

const STEP_GOAL_STEP = 500;
const LOWEST_STEP_GOAL = 1000;
const HIGHEST_STEP_GOAL = 50000;

// ============================================================================
// Derived values
// ============================================================================

const brightness_name = computed(() => BRIGHTNESS_NAMES[brightness.value - 1]);

const timeout_text = computed(() => `${settings.screen_timeout_seconds.value} s`);

const chime_name = computed(() => CHIME_NAMES[settings.chimes.value]);

const wake_name = computed(() => WAKE_NAMES[settings.wake_on_touch.value]);

const theme_name = computed(() => (settings.theme.value < 0 ? "Own" : THEME_NAMES[settings.theme.value]));

// ============================================================================
// Changes
// ============================================================================

export function set_brightness_level(level: number) {
  set_setting("brightness", level);
}

export function set_timeout(seconds: number) {
  set_setting("screen_timeout_seconds", seconds);
}

export function toggle_24_hour_clock() {
  set_setting("uses_24_hour_clock", settings.uses_24_hour_clock.value ? 0 : 1);
}

export function toggle_metric_units() {
  set_setting("uses_metric_units", settings.uses_metric_units.value ? 0 : 1);
}

export function toggle_always_on_screen() {
  set_setting("always_on_screen", settings.always_on_screen.value ? 0 : 1);
}

export function toggle_do_not_disturb() {
  set_do_not_disturb(!is_do_not_disturb.value);
}

export function toggle_alarms_in_do_not_disturb() {
  set_setting("alarms_in_do_not_disturb", settings.alarms_in_do_not_disturb.value ? 0 : 1);
}

export function toggle_bluetooth() {
  set_bluetooth(!is_bluetooth_enabled.value);
}

export function toggle_updates() {
  set_setting("allows_updates", settings.allows_updates.value ? 0 : 1);
}

// ============================================================================
// Screens
// ============================================================================

/** A row that opens a screen of choices, showing the current one on the right. */
function ChoiceRow(props: { title: string; value: string; onClick: () => void }) {
  return (
    <Row gap={8} padding={8} height={48} background="surface" radius={12} onClick={props.onClick}>
      <Label grow={1}>{props.title}</Label>
      <Label color="accent">{props.value}</Label>
    </Row>
  );
}

/** A row with a switch on the right. */
function SwitchRow(props: { title: string; value: boolean; onChange: () => void }) {
  return (
    <Row gap={8} padding={8} height={48} background="surface" radius={12}>
      <Label grow={1}>{props.title}</Label>
      <Switch value={props.value} onChange={props.onChange} />
    </Row>
  );
}

function SettingsList() {
  return (
    <Screen padding={8} gap={6}>
      <Row gap={6} height={28}>
        <Icon icon={icons.settings} size={22} color="accent" />
        <Label color="muted">Settings</Label>
      </Row>
      <Scroll grow={1} gap={6}>
        <ChoiceRow title="Brightness" value={brightness_name.value} onClick={() => go("brightness")} />
        <ChoiceRow title="Screen off" value={timeout_text.value} onClick={() => go("timeout")} />
        <SwitchRow title="Always on" value={settings.always_on_screen.value} onChange={toggle_always_on_screen} />
        <ChoiceRow title="Wake" value={wake_name.value} onClick={() => go("wake")} />
        <SwitchRow title="24 hour clock" value={settings.uses_24_hour_clock.value} onChange={toggle_24_hour_clock} />
        <SwitchRow title="Metric units" value={settings.uses_metric_units.value} onChange={toggle_metric_units} />
        <ChoiceRow title="Step goal" value={`${settings.step_goal.value}`} onClick={() => go("step_goal")} />
        <ChoiceRow title="Chimes" value={chime_name.value} onClick={() => go("chimes")} />
        <SwitchRow title="Do not disturb" value={is_do_not_disturb.value} onChange={toggle_do_not_disturb} />
        <SwitchRow
          title="Alarms in DND"
          value={settings.alarms_in_do_not_disturb.value}
          onChange={toggle_alarms_in_do_not_disturb}
        />
        <ChoiceRow title="Theme" value={theme_name.value} onClick={() => go("theme")} />
        <SwitchRow title="Bluetooth" value={is_bluetooth_enabled.value} onChange={toggle_bluetooth} />
        <SwitchRow title="Updates" value={settings.allows_updates.value} onChange={toggle_updates} />
      </Scroll>
    </Screen>
  );
}

/** One choice on a screen of choices, filled in while chosen. */
function Choice(props: { label: string; chosen: boolean; onClick: () => void }) {
  return (
    <Button height={40} background={props.chosen ? "accent" : "surface"} onClick={props.onClick}>
      {props.label}
    </Button>
  );
}

function Brightness() {
  return (
    <Screen padding={8} gap={6}>
      <Label color="muted">Brightness</Label>
      <Column grow={1} gap={6} justify="center">
        {range(3).map((place) => (
          <Choice
            label={BRIGHTNESS_NAMES[place]}
            chosen={brightness.value === place + 1}
            onClick={() => set_brightness_level(place + 1)}
          />
        ))}
      </Column>
    </Screen>
  );
}

function Timeout() {
  return (
    <Screen padding={8} gap={6}>
      <Label color="muted">Screen off after</Label>
      <Grid columns={2} gap={6} grow={1}>
        {range(TIMEOUT_COUNT).map((place) => (
          <Choice
            label={`${TIMEOUTS_IN_SECONDS[place]} s`}
            chosen={settings.screen_timeout_seconds.value === TIMEOUTS_IN_SECONDS[place]}
            onClick={() => set_timeout(TIMEOUTS_IN_SECONDS[place])}
          />
        ))}
      </Grid>
    </Screen>
  );
}

function StepGoal() {
  return (
    <Screen padding={8} gap={6}>
      <Label color="muted">Daily step goal</Label>
      <Column grow={1} align_items="center" justify="center">
        <Icon icon={icons.steps} size={36} color="accent" />
      </Column>
      <Stepper
        height={52}
        gap={6}
        font="large"
        value={settings.step_goal.value}
        minimum={LOWEST_STEP_GOAL}
        maximum={HIGHEST_STEP_GOAL}
        step={STEP_GOAL_STEP}
        onChange={(event) => set_setting("step_goal", event.value)}
      />
    </Screen>
  );
}

function Chimes() {
  return (
    <Screen padding={8} gap={6}>
      <Label color="muted">Chimes</Label>
      <Column grow={1} gap={6} justify="center">
        {range(3).map((place) => (
          <Choice
            label={CHIME_NAMES[place]}
            chosen={settings.chimes.value === place}
            onClick={() => set_setting("chimes", place)}
          />
        ))}
      </Column>
    </Screen>
  );
}

function Wake() {
  return (
    <Screen padding={8} gap={6}>
      <Label color="muted">Wake the screen with</Label>
      <Column grow={1} gap={6} justify="center">
        {range(3).map((place) => (
          <Choice
            label={WAKE_NAMES[place]}
            chosen={settings.wake_on_touch.value === place}
            onClick={() => set_setting("wake_on_touch", place)}
          />
        ))}
      </Column>
    </Screen>
  );
}

function ThemeChoice(props: { place: number }) {
  const place = props.place;
  return (
    <Button
      background={THEME_BACKGROUNDS[place]}
      border_color={THEME_ACCENTS[place]}
      border_width={settings.theme.value === place ? 4 : 2}
      color={THEME_ACCENTS[place]}
      onClick={() => set_setting("theme", place)}
    >
      {THEME_NAMES[place]}
    </Button>
  );
}

function Theme() {
  return (
    <Screen padding={8} gap={6}>
      <Label color="muted">Theme</Label>
      <Grid columns={2} gap={6} grow={1}>
        {range(THEME_COUNT).map((place) => (
          <ThemeChoice place={place} />
        ))}
      </Grid>
    </Screen>
  );
}

export default define_app({
  id: "settings",
  name: "Settings",
  group: "settings",
  icon: icons.settings,
  screens: {
    main: SettingsList,
    brightness: Brightness,
    timeout: Timeout,
    step_goal: StepGoal,
    chimes: Chimes,
    wake: Wake,
    theme: Theme,
  },
  permissions: ["settings"],
});
