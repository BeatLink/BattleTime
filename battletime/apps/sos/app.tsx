// SOS: hold the red button for five seconds to tell the phone you need help, while the light, screen and motor flash SOS.
// The second page chooses those signals and runs them alone; leaving the app stops the signals but not the SOS.
// The phone can stop the SOS too, with a message on the app's own topic, even while the app is closed.

import {
  brightness,
  computed,
  define_app,
  is_phone_connected,
  keep_screen_on,
  now,
  on,
  persisted,
  send_to_phone,
  set_brightness,
  set_heart_rate_led,
  signal,
  vibrate,
  type Color,
  type Instant,
  type PageChangeEvent,
  type TimeEvent,
} from "battletime";
import { Card, ConfirmDialog } from "battletime/components";
import { icons } from "battletime/icons";
import { Arc, Button, Column, Label, Page, Pages, Row, Screen, Switch } from "battletime/widgets";

// ============================================================================
// The SOS pattern
// ============================================================================

/** One Morse unit, the length of a dot, in milliseconds; the app steps once a unit. */
const UNIT_MILLISECONDS = 200;

/** Whether each unit of "... --- ..." is lit: dots 1 unit, dashes 3, gaps of 1, 3 between letters and 7 before it repeats. */
export const SOS_LIT = [
  1, 0, 1, 0, 1, 0, 0, 0,
  1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 0, 0,
  1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0,
];

/** How many units the motor runs from each unit: a dot or dash where one starts, else nothing. */
export const SOS_BUZZ_UNITS = [
  1, 0, 1, 0, 1, 0, 0, 0,
  3, 0, 0, 0, 3, 0, 0, 0, 3, 0, 0, 0, 0, 0,
  1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0,
];

export const SOS_LENGTH = 34;

/** Where the seven dark units before the repeat begin, so the pattern starts after a pause. */
export const REPEAT_GAP_STEP = 27;

// ============================================================================
// State
// ============================================================================

/** How long the button must be held to send the SOS. */
export const HOLD_MILLISECONDS = 5000;

/** The quick buzz each unit while the button is held, and the long one once the SOS is sent. */
const HOLD_BUZZ_MILLISECONDS = 60;
const SENT_BUZZ_MILLISECONDS = 1000;

/** The LED's brightest drive level, held steady while lit. */
const LED_BRIGHTNESS = 3;
const LED_STEADY = 0;
const BRIGHTEST = 3;

const SOS_RED = "#c00000";

/** Whether the SOS is on, kept so reopening the app shows Stop. */
export const is_active = persisted("is_active", false);

/** Whether the phone has heard the SOS; it is sent again when a phone connects. */
export const reached_phone = persisted("reached_phone", false);

export const use_light = persisted("use_light", true);
export const use_screen = persisted("use_screen", true);
export const use_vibration = persisted("use_vibration", true);

/** When the finger landed on the button, or null while it is not held. */
const hold_started_at = signal<Instant | null>(null);

const held_milliseconds = signal(0);

/** Whether the signals run without an SOS, from the Signal button. */
const is_signalling_alone = signal(false);

/** The unit of the pattern the next step shows. */
const pattern_step = signal(0);

const is_lit = signal(false);

const page = signal(0);

const is_asking_to_stop = signal(false);

// ============================================================================
// Derived values
// ============================================================================

const is_holding = computed(() => hold_started_at.value !== null);

const is_signalling = computed(() => is_active.value || is_signalling_alone.value);

const is_flashing_screen = computed(() => is_signalling.value && use_screen.value);

const seconds_left = computed(() => Math.floor((HOLD_MILLISECONDS - held_milliseconds.value + 999) / 1000));

/** White when lit and black when dark while the screen flashes, red while the SOS is on, else the theme. */
const background = computed<Color>(() => {
  if (is_flashing_screen.value) return is_lit.value ? "#ffffff" : "#000000";
  return is_active.value ? SOS_RED : "background";
});

const foreground = computed<Color>(() => {
  if (is_flashing_screen.value) return is_lit.value ? "#000000" : "#ffffff";
  return is_active.value ? "#ffffff" : "text";
});

// ============================================================================
// Signals
// ============================================================================

function start_signals(first_step: number) {
  pattern_step.value = first_step;
  is_lit.value = false;
  apply_signal_choices();
}

function stop_signals() {
  is_lit.value = false;
  set_heart_rate_led(false, LED_BRIGHTNESS, LED_STEADY);
  keep_screen_on(false);
  set_brightness(brightness.value);
}

/** Brings the light, screen and backlight in line with the switches while signalling. */
function apply_signal_choices() {
  if (!is_signalling.value) return;
  set_heart_rate_led(use_light.value && is_lit.value, LED_BRIGHTNESS, LED_STEADY);
  keep_screen_on(use_screen.value);
  set_brightness(use_screen.value ? BRIGHTEST : brightness.value);
}

/** Shows one unit of the pattern on every chosen signal, then moves to the next. */
function step_pattern() {
  const lit = SOS_LIT[pattern_step.value] === 1;
  if (lit !== is_lit.value) {
    is_lit.value = lit;
    if (use_light.value) set_heart_rate_led(lit, LED_BRIGHTNESS, LED_STEADY);
  }
  const buzz_units = SOS_BUZZ_UNITS[pattern_step.value];
  if (buzz_units > 0 && use_vibration.value) vibrate(buzz_units * UNIT_MILLISECONDS);
  pattern_step.value = (pattern_step.value + 1) % SOS_LENGTH;
}

function toggle_signalling_alone() {
  is_signalling_alone.value = !is_signalling_alone.value;
  if (is_signalling_alone.value) {
    start_signals(0);
  } else {
    stop_signals();
  }
}

// ============================================================================
// Holding and sending
// ============================================================================

function start_hold() {
  hold_started_at.value = now.value;
  held_milliseconds.value = 0;
  vibrate(HOLD_BUZZ_MILLISECONDS);
}

function cancel_hold() {
  hold_started_at.value = null;
  held_milliseconds.value = 0;
}

function send_if_connected() {
  if (!is_phone_connected.value) return;
  send_to_phone("sos", { on: 1 });
  reached_phone.value = true;
}

function complete_hold() {
  cancel_hold();
  is_active.value = true;
  reached_phone.value = false;
  send_if_connected();
  vibrate(SENT_BUZZ_MILLISECONDS);
  start_signals(REPEAT_GAP_STEP);
}

function continue_hold(time: Instant) {
  if (hold_started_at.value === null) return;
  const held = time - hold_started_at.value;
  if (held >= HOLD_MILLISECONDS) {
    complete_hold();
  } else {
    held_milliseconds.value = held;
    vibrate(HOLD_BUZZ_MILLISECONDS);
  }
}

/** Ends the SOS on the watch alone, leaving the phone untold. */
function end_sos() {
  is_active.value = false;
  reached_phone.value = false;
  is_asking_to_stop.value = false;
  if (!is_signalling_alone.value) stop_signals();
}

function stop_sos() {
  send_to_phone("sos", { on: 0 });
  end_sos();
}

function ask_to_stop() {
  is_asking_to_stop.value = true;
}

// ============================================================================
// Events
// ============================================================================

function step(event: TimeEvent) {
  if (is_holding.value) continue_hold(event.time);
  if (is_signalling.value) step_pattern();
}

function turn_page(event: PageChangeEvent) {
  cancel_hold();
  page.value = event.page;
}

function open() {
  page.value = 0;
  if (is_active.value) {
    if (!reached_phone.value) send_if_connected();
    start_signals(REPEAT_GAP_STEP);
  }
}

/** The runtime ends the light, backlight and screen hold on leaving; the SOS itself stays on. */
function close() {
  cancel_hold();
  is_signalling_alone.value = false;
  is_asking_to_stop.value = false;
  is_lit.value = false;
}

function phone_connected() {
  if (is_active.value && !reached_phone.value) send_if_connected();
}

/** The phone stopped the SOS with `{ "on": 0 }`, so the watch stops it without telling the phone back; `{ "on": 1 }` changes nothing. */
function heard_from_phone(message: { on: number }) {
  if (message.on === 0 && is_active.value) end_sos();
}

// ============================================================================
// Pages
// ============================================================================

function HoldButton() {
  return (
    <Column grow={1} align_items="center" justify="center" gap={4}>
      <Label font="small" color={is_holding.value ? foreground.value : "muted"}>
        {is_holding.value ? "Keep holding" : "Hold for 5 seconds"}
      </Label>
      <Arc
        diameter={180}
        thickness={10}
        start_angle={0}
        end_angle={360}
        value={held_milliseconds.value}
        minimum={0}
        maximum={HOLD_MILLISECONDS}
        color="danger"
      >
        <Button
          width={150}
          height={150}
          radius={75}
          background={SOS_RED}
          onPress={start_hold}
          onRelease={cancel_hold}
        >
          <Label font="huge" color="#ffffff" width={150} text_align="center">
            {is_holding.value ? `${seconds_left.value}` : "SOS"}
          </Label>
        </Button>
      </Arc>
    </Column>
  );
}

function ActiveSos() {
  return (
    <Column grow={1} padding={8} gap={6} align_items="center">
      <Column grow={1} align_items="center" justify="center" gap={6}>
        <Label font="large" color={foreground.value}>
          {reached_phone.value ? "SOS sent" : "SOS on"}
        </Label>
        {!reached_phone.value && (
          <Label font="small" color={foreground.value}>
            Phone not connected
          </Label>
        )}
        {!reached_phone.value && (
          <Label font="small" color={foreground.value}>
            Signalling only
          </Label>
        )}
      </Column>
      <Button width="100%" height={56} background="surface" onClick={ask_to_stop}>
        Stop
      </Button>
    </Column>
  );
}

function SosPage() {
  return (
    <Page>
      {!is_active.value && <HoldButton />}
      {is_active.value && <ActiveSos />}
    </Page>
  );
}

function SignalsPage() {
  return (
    <Page padding={8} gap={6}>
      <Card padding={6}>
        <Row justify="space_between" align_items="center">
          <Label>Light</Label>
          <Switch value={use_light} onChange={apply_signal_choices} />
        </Row>
      </Card>
      <Card padding={6}>
        <Row justify="space_between" align_items="center">
          <Label>Screen</Label>
          <Switch value={use_screen} onChange={apply_signal_choices} />
        </Row>
      </Card>
      <Card padding={6}>
        <Row justify="space_between" align_items="center">
          <Label>Vibrate</Label>
          <Switch value={use_vibration} onChange={apply_signal_choices} />
        </Row>
      </Card>
      <Button
        grow={1}
        background={is_signalling_alone.value ? "danger" : "accent"}
        disabled={is_active.value}
        onClick={toggle_signalling_alone}
      >
        {is_signalling_alone.value ? "Stop" : "Signal"}
      </Button>
    </Page>
  );
}

// ============================================================================
// Screen
// ============================================================================

function Sos() {
  return (
    <Screen background={background.value}>
      <Pages grow={1} value={page.value} onPageChange={turn_page}>
        <SosPage />
        <SignalsPage />
      </Pages>
      <ConfirmDialog open={is_asking_to_stop} question="Stop the SOS?" onConfirm={stop_sos} />
    </Screen>
  );
}

on("open", open);
on("close", close);
on("tick", step, { every: { milliseconds: UNIT_MILLISECONDS }, while_open: true });
on("phone_connects", phone_connected, { while_open: true });
on("message", heard_from_phone);

export default define_app({
  id: "sos",
  name: "SOS",
  group: "phone",
  icon: icons.bell,
  screens: { main: Sos },
});
