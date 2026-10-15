// Settings, merged from InfiniTime's Settings and wasp-os's settings app.
// From InfiniTime: one list of what can be changed, each opening its own screen of choices, the
// display timeout, the 12 or 24 hour clock, the step goal, the hourly chimes and how often the heart
// rate is measured in the background.
// From wasp-os: brightness in three steps, units, and do not disturb.
// Also from InfiniTime: raise to wake, lower to sleep and shake to wake, with a screen that sets how
// hard a shake must be by dragging an arc or by recording a shake, and sleep mode.
// Also: what touch wakes the screen, whether alarms ring in do not disturb, Bluetooth until the next
// restart, and whether the watch takes firmware updates, with InfiniTime's "until restart" choice.
// Also from InfiniTime: choosing the watch face, here by stepping through the faces turned on in Software.
// Also from InfiniTime: setting the date and the time by hand, on two pages, for when no phone has.
// The theme is one of the watch's presets; the companion app can also set colours of the user's own.
// Every change goes to the watch's settings with `set_setting`, so the companion app sees it too.

import {
  back,
  brightness,
  choose_watch_face,
  computed,
  day_of,
  define_app,
  go,
  hour_of,
  is_bluetooth_enabled,
  is_do_not_disturb,
  minute_of,
  month_of,
  now,
  on,
  range,
  set_bluetooth,
  set_clock,
  set_do_not_disturb,
  set_setting,
  settings,
  shake_speed,
  signal,
  watch_face_name,
  type Color,
  type Instant,
  year_of,
} from "battletime";
import { icons } from "battletime/icons";
import { days_in_year } from "battletime/lib";
import {
  Arc,
  Button,
  Column,
  Grid,
  Icon,
  Label,
  NumberRoller,
  Page,
  Pages,
  Roller,
  Row,
  Screen,
  Scroll,
  Stepper,
  Switch,
  TimeRollers,
} from "battletime/widgets";

// ============================================================================
// Choices
// ============================================================================

const BRIGHTNESS_NAMES = ["Low", "Medium", "High"];

const TIMEOUTS_IN_SECONDS = [5, 10, 15, 30, 60, 120];
const TIMEOUT_COUNT = 6;

const CHIME_NAMES = ["Off", "Every hour", "Every half hour"];

const WAKE_NAMES = ["Button only", "Tap", "Double tap"];

/** Whether the watch takes updates, in the order the watch numbers them, Temporary lasting until the next restart. */
const UPDATES_NAMES = ["Off", "On", "Temporary"];
const UPDATES_CHOICE_COUNT = 3;

/** InfiniTime's background heart rate choices, in the order the watch numbers them. */
const HEART_RATE_INTERVAL_NAMES = ["Off", "Always", "30 s", "1 min", "5 min", "10 min", "30 min"];
const HEART_RATE_INTERVAL_COUNT = 7;

/** The watch's preset themes in its own order, each shown by its background and accent colours. */
const THEME_NAMES = ["Dark", "Light", "Ocean", "Forest", "Sunset", "Contrast"];
const THEME_BACKGROUNDS: Color[] = ["#000000", "#ffffff", "#000a1e", "#08140a", "#180810", "#000000"];
const THEME_ACCENTS: Color[] = ["#00aaa0", "#008096", "#28a0ff", "#78c850", "#ff783c", "#ffdc00"];
const THEME_COUNT = 6;

/** InfiniTime's shake speeds, which the threshold and the live motion arc are drawn against. */
const HIGHEST_SHAKE_THRESHOLD = 4095;

/** A recorded shake sets the threshold this far below its peak, as InfiniTime does, so a slightly weaker shake still wakes. */
const CALIBRATION_MARGIN = 300;

/** How long the wearer gets ready, then shakes, while a shake is recorded. */
const GET_READY_MILLISECONDS = 2000;
const SHAKE_MILLISECONDS = 7500;

/** How often the shake speed is looked at while a shake is recorded. */
const SHAKE_SAMPLE_MILLISECONDS = 100;

const CALIBRATION_IDLE = 0;
const CALIBRATION_GETTING_READY = 1;
const CALIBRATION_SHAKING = 2;
const CALIBRATE_LABELS = ["Calibrate", "Ready!", "Shake!"];
const CALIBRATE_COLORS: Color[] = ["surface", "accent", "danger"];

const STEP_GOAL_STEP = 500;
const LOWEST_STEP_GOAL = 1000;
const HIGHEST_STEP_GOAL = 50000;

const MONTH_NAMES = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const FEBRUARY = 2;
const DAYS_IN_LEAP_YEAR = 366;
const DAYS_OUTSIDE_FEBRUARY = 337;

const EARLIEST_SETTABLE_YEAR = 2000;
const LATEST_SETTABLE_YEAR = 2099;

// ============================================================================
// Derived values
// ============================================================================

const brightness_name = computed(() => BRIGHTNESS_NAMES[brightness.value - 1]);

const timeout_text = computed(() => `${settings.screen_timeout_seconds.value} s`);

const chime_name = computed(() => CHIME_NAMES[settings.chimes.value]);

const wake_name = computed(() => WAKE_NAMES[settings.wake_on_touch.value]);

const heart_rate_interval_name = computed(() => HEART_RATE_INTERVAL_NAMES[settings.heart_rate_interval.value]);

const theme_name = computed(() => (settings.theme.value < 0 ? "Own" : THEME_NAMES[settings.theme.value]));

const updates_name = computed(() => UPDATES_NAMES[settings.allows_updates.value]);

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

export function toggle_sleep_mode() {
  set_setting("sleep_mode", settings.sleep_mode.value ? 0 : 1);
}

export function toggle_raise_to_wake() {
  set_setting("raise_to_wake", settings.raise_to_wake.value ? 0 : 1);
}

export function toggle_lower_to_sleep() {
  set_setting("lower_to_sleep", settings.lower_to_sleep.value ? 0 : 1);
}

export function toggle_shake_to_wake() {
  set_setting("shake_to_wake", settings.shake_to_wake.value ? 0 : 1);
}

export function toggle_alarms_in_do_not_disturb() {
  set_setting("alarms_in_do_not_disturb", settings.alarms_in_do_not_disturb.value ? 0 : 1);
}

/** Steps the updates choice on: off, on, then on only until the next restart. */
export function step_updates() {
  set_setting("allows_updates", (settings.allows_updates.value + 1) % UPDATES_CHOICE_COUNT);
}

export function toggle_bluetooth() {
  set_bluetooth(!is_bluetooth_enabled.value);
}


// ============================================================================
// Shake calibration, from InfiniTime's SettingShakeThreshold
// ============================================================================

/** The threshold the arc shows and drags, saved whenever it is let go or a recorded shake ends. */
export const chosen_shake_threshold = signal(150);

export const calibration_stage = signal(CALIBRATION_IDLE);

/** When getting ready or shaking ends, while a shake is recorded. */
export const calibration_stage_ends_at = signal<Instant | null>(null);

/** When the shake speed is next looked at, while the wearer shakes. */
export const next_shake_sample_at = signal<Instant | null>(null);

export function open_shake_calibration() {
  chosen_shake_threshold.value = settings.shake_threshold.value;
  calibration_stage.value = CALIBRATION_IDLE;
  go("shake_threshold");
}

export function set_shake_threshold(threshold: number) {
  chosen_shake_threshold.value = threshold;
  set_setting("shake_threshold", threshold);
}

/** Starts recording a shake, or stops one being recorded and keeps what it found so far. */
export function calibrate() {
  if (calibration_stage.value === CALIBRATION_IDLE) {
    chosen_shake_threshold.value = 0;
    calibration_stage.value = CALIBRATION_GETTING_READY;
    calibration_stage_ends_at.value = now.value + GET_READY_MILLISECONDS;
  } else {
    finish_calibration();
  }
}

export function next_calibration_stage() {
  if (calibration_stage.value === CALIBRATION_GETTING_READY) {
    calibration_stage.value = CALIBRATION_SHAKING;
    calibration_stage_ends_at.value = now.value + SHAKE_MILLISECONDS;
    next_shake_sample_at.value = now.value + SHAKE_SAMPLE_MILLISECONDS;
  } else {
    finish_calibration();
  }
}

/** Raises the threshold to just below the hardest shake so far. */
export function sample_shake() {
  if (calibration_stage.value !== CALIBRATION_SHAKING) return;
  const reached = shake_speed.value - CALIBRATION_MARGIN;
  if (reached > chosen_shake_threshold.value) chosen_shake_threshold.value = Math.min(reached, HIGHEST_SHAKE_THRESHOLD);
  next_shake_sample_at.value = now.value + SHAKE_SAMPLE_MILLISECONDS;
}

function finish_calibration() {
  calibration_stage.value = CALIBRATION_IDLE;
  calibration_stage_ends_at.value = null;
  next_shake_sample_at.value = null;
  set_setting("shake_threshold", chosen_shake_threshold.value);
}

// ============================================================================
// Date and time, from InfiniTime's SettingSetDateTime
// ============================================================================

export const chosen_year = signal(2026);
export const chosen_month = signal(1);
export const chosen_day = signal(1);
export const chosen_minute_of_day = signal(0);

/** How many days the chosen month has: February what the year leaves, the others 31 for odd months until July and even ones from August. */
export const chosen_month_length = computed(() =>
  chosen_month.value === FEBRUARY
    ? days_in_year(chosen_year.value) - DAYS_OUTSIDE_FEBRUARY
    : 30 + ((chosen_month.value ^ (chosen_month.value >> 3)) & 1),
);

/** Opens the date and time pages on the date, starting from the local time now. */
export function open_date_time() {
  chosen_year.value = Math.min(Math.max(year_of(now.value), EARLIEST_SETTABLE_YEAR), LATEST_SETTABLE_YEAR);
  chosen_month.value = month_of(now.value);
  chosen_day.value = day_of(now.value);
  chosen_minute_of_day.value = hour_of(now.value) * 60 + minute_of(now.value);
  go("clock");
}

/** Sets the watch's clock to the chosen date and time at 00 seconds, then goes back to the list. */
export function save_date_time() {
  const days_february_falls_short = days_in_year(chosen_year.value) === DAYS_IN_LEAP_YEAR ? 1 : 2;
  const month = chosen_month.value;
  // The day roller shows a day past the month's end as its last day, such as 31 April as 30 April.
  const day = Math.min(chosen_day.value, chosen_month_length.value);
  // The usual calendar formula for the day of the year, which takes February's shortfall off from March on.
  const day_of_year =
    Math.floor((275 * month) / 9) - days_february_falls_short * Math.floor((month + 9) / 12) + day - 30;
  set_clock(chosen_year.value, day_of_year, chosen_minute_of_day.value);
  back();
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
      <Scroll grow={1} gap={6}>
        <ChoiceRow title="Brightness" value={brightness_name.value} onClick={() => go("brightness")} />
        <ChoiceRow title="Screen off" value={timeout_text.value} onClick={() => go("timeout")} />
        <SwitchRow title="Always on" value={settings.always_on_screen.value} onChange={toggle_always_on_screen} />
        <ChoiceRow title="Wake" value={wake_name.value} onClick={() => go("wake")} />
        <SwitchRow title="24 hour" value={settings.uses_24_hour_clock.value} onChange={toggle_24_hour_clock} />
        <SwitchRow title="Metric" value={settings.uses_metric_units.value} onChange={toggle_metric_units} />
        <ChoiceRow title="Step goal" value={`${settings.step_goal.value}`} onClick={() => go("step_goal")} />
        <ChoiceRow title="Chimes" value={chime_name.value} onClick={() => go("chimes")} />
        <ChoiceRow
          title="Heart rate"
          value={heart_rate_interval_name.value}
          onClick={() => go("heart_rate_interval")}
        />
        <SwitchRow title="DND" value={is_do_not_disturb.value} onChange={toggle_do_not_disturb} />
        <SwitchRow title="Sleep mode" value={settings.sleep_mode.value} onChange={toggle_sleep_mode} />
        <SwitchRow
          title="DND alarms"
          value={settings.alarms_in_do_not_disturb.value}
          onChange={toggle_alarms_in_do_not_disturb}
        />
        <ChoiceRow title="Face" value={watch_face_name.value} onClick={() => choose_watch_face()} />
        <ChoiceRow title="Theme" value={theme_name.value} onClick={() => go("theme")} />
        <SwitchRow title="Bluetooth" value={is_bluetooth_enabled.value} onChange={toggle_bluetooth} />
        <ChoiceRow title="Updates" value={updates_name.value} onClick={step_updates} />
        <Row padding={8} height={48} background="surface" radius={12} onClick={open_date_time}>
          <Label grow={1}>Date & time</Label>
        </Row>
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

function HeartRateInterval() {
  return (
    <Screen padding={8} gap={6}>
      <Label color="muted">Measure heart rate</Label>
      <Grid columns={2} gap={6} grow={1}>
        {range(HEART_RATE_INTERVAL_COUNT).map((place) => (
          <Choice
            label={HEART_RATE_INTERVAL_NAMES[place]}
            chosen={settings.heart_rate_interval.value === place}
            onClick={() => set_setting("heart_rate_interval", place)}
          />
        ))}
      </Grid>
    </Screen>
  );
}

function Wake() {
  return (
    <Screen padding={8} gap={6}>
      <Scroll grow={1} gap={6}>
        <ChoiceRow title="Touch" value={wake_name.value} onClick={() => go("wake_touch")} />
        <SwitchRow title="Raise wrist" value={settings.raise_to_wake.value} onChange={toggle_raise_to_wake} />
        <SwitchRow title="Shake" value={settings.shake_to_wake.value} onChange={toggle_shake_to_wake} />
        <ChoiceRow
          title="Shake strength"
          value={`${settings.shake_threshold.value}`}
          onClick={open_shake_calibration}
        />
        <SwitchRow title="Lower to sleep" value={settings.lower_to_sleep.value} onChange={toggle_lower_to_sleep} />
      </Scroll>
    </Screen>
  );
}

function WakeTouch() {
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

/** The threshold as an outer arc the finger drags, the wrist's motion now as an inner arc, and a button that records a shake. */
function ShakeThreshold() {
  return (
    <Screen>
      <Label x={0} y={4} width={240} text_align="center" color="muted">
        Shake to wake
      </Label>
      <Arc
        x={10}
        y={30}
        diameter={220}
        thickness={14}
        start_angle={270}
        end_angle={90}
        minimum={0}
        maximum={HIGHEST_SHAKE_THRESHOLD}
        value={chosen_shake_threshold}
        onChange={(event) => set_shake_threshold(event.value)}
      />
      <Arc
        x={30}
        y={50}
        diameter={180}
        thickness={8}
        start_angle={270}
        end_angle={90}
        minimum={0}
        maximum={HIGHEST_SHAKE_THRESHOLD}
        value={shake_speed.value}
        color="danger"
      />
      <Label x={0} y={100} width={240} text_align="center" font="large">
        {`${chosen_shake_threshold.value}`}
      </Label>
      <Button
        x={20}
        y={166}
        width={200}
        height={64}
        background={CALIBRATE_COLORS[calibration_stage.value]}
        onClick={calibrate}
      >
        {CALIBRATE_LABELS[calibration_stage.value]}
      </Button>
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

/** The date as day, month and year rollers, then the time as hour and minute rollers, each on its own page. */
function DateTime() {
  return (
    <Screen>
      <Pages grow={1}>
        <Page padding={8} gap={6}>
          <Row grow={1} gap={4} align_items="center" justify="center">
            <NumberRoller
              font="normal"
              value={chosen_day.value}
              minimum={1}
              maximum={chosen_month_length.value}
              digits={2}
              onChange={(event) => (chosen_day.value = event.value)}
            />
            <Roller
              font="normal"
              options={MONTH_NAMES}
              value={chosen_month.value - 1}
              onChange={(event) => (chosen_month.value = event.value + 1)}
            />
            <NumberRoller
              font="normal"
              value={chosen_year.value}
              minimum={EARLIEST_SETTABLE_YEAR}
              maximum={LATEST_SETTABLE_YEAR}
              onChange={(event) => (chosen_year.value = event.value)}
            />
          </Row>
        </Page>
        <Page padding={8} gap={6}>
          <TimeRollers
            grow={1}
            value={chosen_minute_of_day.value}
            twelve_hour={!settings.uses_24_hour_clock.value}
            onChange={(event) => (chosen_minute_of_day.value = event.value)}
          />
          <Button height={48} background="accent" onClick={save_date_time}>
            Set
          </Button>
        </Page>
      </Pages>
    </Screen>
  );
}

on("time", next_calibration_stage, { at: calibration_stage_ends_at, while_open: true });
on("time", sample_shake, { at: next_shake_sample_at, while_open: true });

export default define_app({
  id: "settings",
  name: "Settings",
  group: "system",
  icon: icons.settings,
  screens: {
    main: SettingsList,
    brightness: Brightness,
    timeout: Timeout,
    step_goal: StepGoal,
    chimes: Chimes,
    heart_rate_interval: HeartRateInterval,
    wake: Wake,
    wake_touch: WakeTouch,
    shake_threshold: ShakeThreshold,
    theme: Theme,
    clock: DateTime,
  },
});

