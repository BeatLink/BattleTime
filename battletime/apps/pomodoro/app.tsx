// Pomodoro timer, after wasp-os's pomodoro app.
// Work and short breaks take turns, with a long break in place of every Nth short one. Three pages set
// the work, short break and long break lengths, and the last page runs the countdown, buzzing between
// each interval and going on until stopped.

import {
  computed,
  define_app,
  format_duration,
  now,
  on,
  persisted,
  signal,
  stop_vibrating,
  vibrate,
  vibrate_pattern,
  wake_screen,
  MINUTE,
  SECOND,
  type Instant,
  type PageChangeEvent,
  type TimeEvent,
} from "battletime";
import { Card, Header } from "battletime/components";
import { icons, type LibraryIcon } from "battletime/icons";
import { Button, Icon, Label, Page, Pages, Row, Screen, Stepper } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

const SHORTEST_MINUTES = 1;
const LONGEST_MINUTES = 99;
const FEWEST_WORKS_PER_LONG_BREAK = 2;
const MOST_WORKS_PER_LONG_BREAK = 8;

/** How long the buzz between intervals lasts before the next one starts. */
const BUZZ_LENGTH = 10 * SECOND;

/** The kinds of interval, in the order the wearer sets their lengths. */
const WORK = 0;
const SHORT_BREAK = 1;
const LONG_BREAK = 2;

const INTERVAL_NAMES = ["Work", "Short break", "Long break"];

/** The countdown is the last page, and the app opens on it. */
const COUNTDOWN_PAGE = 3;

export const work_minutes = persisted("work_minutes", 25);
export const short_break_minutes = persisted("short_break_minutes", 5);
export const long_break_minutes = persisted("long_break_minutes", 15);

/** How many work intervals come before each long break. */
export const works_per_long_break = persisted("works_per_long_break", 4);

/** Which kind of interval runs now, or ran last. */
export const interval = persisted("interval", WORK);

/** How many work intervals have finished since the last long break. */
export const works_done = persisted("works_done", 0);

/** When the running interval ends, or null while none runs. */
export const ends_at = persisted<Instant | null>("ends_at", null);

/** When the last interval ended and the buzz began, or null while not buzzing. */
export const rang_at = persisted<Instant | null>("rang_at", null);

const page = signal(COUNTDOWN_PAGE);

// ============================================================================
// Derived values
// ============================================================================

const is_running = computed(() => ends_at.value !== null);

const is_ringing = computed(() => rang_at.value !== null);

const is_stopped = computed(() => !is_running.value && !is_ringing.value);

/** The interval that follows the running one: a long break after every Nth work, else a short one. */
const next_interval = computed(() => {
  if (interval.value !== WORK) return WORK;
  return works_done.value + 1 >= works_per_long_break.value ? LONG_BREAK : SHORT_BREAK;
});

const remaining = computed(() =>
  ends_at.value !== null ? Math.floor((Math.max(0, ends_at.value - now.value) + SECOND - 1) / SECOND) * SECOND : 0,
);

/** Which work interval of the round this is, or the one a break follows, such as "2 of 4". */
const round_place = computed(() => {
  if (interval.value === WORK) return works_done.value + 1;
  if (interval.value === SHORT_BREAK) return works_done.value;
  return works_per_long_break.value;
});

const round_text = computed(() => `${round_place.value} of ${works_per_long_break.value}`);

const plan_text = computed(
  () => `${work_minutes.value} / ${short_break_minutes.value} / ${long_break_minutes.value} min`,
);

const long_break_text = computed(() => `After ${works_per_long_break.value} work sessions`);

/** When the buzz ends and the next interval starts. */
const next_interval_at = computed(() => (rang_at.value !== null ? rang_at.value + BUZZ_LENGTH : null));

function minutes_of(kind: number): number {
  if (kind === WORK) return work_minutes.value;
  if (kind === SHORT_BREAK) return short_break_minutes.value;
  return long_break_minutes.value;
}

// ============================================================================
// Handlers
// ============================================================================

/** Every opening starts on the countdown. */
function show_countdown() {
  page.value = COUNTDOWN_PAGE;
}

function turn_page(event: PageChangeEvent) {
  page.value = event.page;
}

export function start() {
  interval.value = WORK;
  works_done.value = 0;
  ends_at.value = now.value + work_minutes.value * MINUTE;
}

export function stop() {
  ends_at.value = null;
  rang_at.value = null;
  interval.value = WORK;
  works_done.value = 0;
  stop_vibrating();
}

function add_minute() {
  if (ends_at.value !== null) {
    ends_at.value = ends_at.value + MINUTE;
    vibrate(50);
  }
}

function ring(event: TimeEvent) {
  rang_at.value = event.time;
  ends_at.value = null;
  wake_screen();
  vibrate_pattern("alarm");
}

function start_next_interval(event: TimeEvent) {
  stop_vibrating();
  rang_at.value = null;
  const next = next_interval.value;
  if (interval.value === WORK) works_done.value = next === LONG_BREAK ? 0 : works_done.value + 1;
  interval.value = next;
  ends_at.value = event.time + minutes_of(next) * MINUTE;
}

// ============================================================================
// Pages
// ============================================================================

function LengthPage(props: {
  title: string;
  icon: LibraryIcon;
  minutes: number;
  onChange: (event: { value: number }) => void;
}) {
  return (
    <Page padding={8} gap={10}>
      <Header title={props.title} icon={props.icon} />
      <Stepper
        height={52}
        gap={6}
        font="large"
        value={props.minutes}
        minimum={SHORTEST_MINUTES}
        maximum={LONGEST_MINUTES}
        onChange={props.onChange}
      />
      <Label font="small" color="muted">Minutes</Label>
    </Page>
  );
}

function LongBreakPage() {
  return (
    <Page padding={8} gap={6}>
      <Header title="Long break" icon={icons.moon} />
      <Stepper
        height={44}
        gap={6}
        font="large"
        value={long_break_minutes.value}
        minimum={SHORTEST_MINUTES}
        maximum={LONGEST_MINUTES}
        onChange={(event) => (long_break_minutes.value = event.value)}
      />
      <Label font="small" color="muted">{long_break_text.value}</Label>
      <Stepper
        height={44}
        gap={6}
        value={works_per_long_break.value}
        minimum={FEWEST_WORKS_PER_LONG_BREAK}
        maximum={MOST_WORKS_PER_LONG_BREAK}
        onChange={(event) => (works_per_long_break.value = event.value)}
      />
    </Page>
  );
}

function CountdownPage() {
  return (
    <Page padding={8} gap={6}>
      <Row justify="space_between" height={36} padding={4}>
        <Label color="accent">{is_stopped.value ? "Pomodoro" : INTERVAL_NAMES[interval.value]}</Label>
        <Label font="small" color="muted">{is_stopped.value ? "" : round_text.value}</Label>
      </Row>

      {is_stopped.value && (
        <Card grow={1} align_items="center" justify="center" gap={6} padding={0}>
          <Label font="condensed">{format_duration(work_minutes.value * MINUTE, "mm:ss")}</Label>
          <Label font="small" color="muted">{plan_text.value}</Label>
        </Card>
      )}

      {is_running.value && (
        <Card grow={1} align_items="center" justify="center" padding={0}>
          <Label font="condensed">{format_duration(remaining.value, "mm:ss")}</Label>
        </Card>
      )}

      {is_ringing.value && (
        <Card grow={1} align_items="center" justify="center" gap={6} padding={0}>
          <Icon icon={icons.timer} size={48} color="warning" />
          <Label>{`${INTERVAL_NAMES[next_interval.value]} next`}</Label>
        </Card>
      )}

      <Row gap={6} height={52}>
        {is_stopped.value && (
          <Button grow={1} height="100%" background="accent" onClick={start}>
            Start
          </Button>
        )}
        {!is_stopped.value && (
          <Button grow={2} height="100%" background="danger" onClick={stop}>
            Stop
          </Button>
        )}
        {is_running.value && (
          <Button grow={1} height="100%" onClick={add_minute}>
            +1
          </Button>
        )}
      </Row>
    </Page>
  );
}

// ============================================================================
// Screen
// ============================================================================

function Pomodoro() {
  return (
    <Screen>
      <Pages grow={1} value={page.value} onPageChange={turn_page}>
        <LengthPage
          title="Work"
          icon={icons.timer}
          minutes={work_minutes.value}
          onChange={(event) => (work_minutes.value = event.value)}
        />
        <LengthPage
          title="Short break"
          icon={icons.sun}
          minutes={short_break_minutes.value}
          onChange={(event) => (short_break_minutes.value = event.value)}
        />
        <LongBreakPage />
        <CountdownPage />
      </Pages>
    </Screen>
  );
}

on("open", show_countdown);
on("time", ring, { at: ends_at });
on("time", start_next_interval, { at: next_interval_at });

export default define_app({
  id: "pomodoro",
  name: "Pomodoro",
  group: "time",
  icon: icons.timer,
  screens: { main: Pomodoro },
});
