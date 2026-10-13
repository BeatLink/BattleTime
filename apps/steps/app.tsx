// Step counter, merged from InfiniTime's Steps and wasp-os's step_counter and steplogger.
// The main screen is InfiniTime's goal ring with yesterday's count and a resettable trip meter,
// turning green once the goal is met as wasp-os's bar does. Swipe left for the week, and again, or tap
// a day, for wasp-os's graph of a day by hour. The system keeps the history, so nothing here runs in
// the background.

import { computed, signal } from "@preact/signals";
import {
  define_app,
  format_time,
  now,
  range,
  reset_step_trip,
  settings,
  step_history,
  steps_today,
  DAY,
  STEP_HISTORY_DAYS,
  type PageChangeEvent,
} from "neotime";
import { Header } from "neotime/components";
import { icons } from "neotime/icons";
import { Arc, Button, Chart, Column, Label, Page, Pages, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const TRIP_LIMIT = 99999;
const HOURS_PER_DAY = 24;

/** Days back from today that the week shows, oldest first. */
const DAYS_SHOWN = [6, 5, 4, 3, 2, 1, 0];

const DAY_PAGE = 2;

/** The page on show: today, the week, then one day. */
const page = signal(0);

/** The day the day page shows, counted back from today. */
const shown_days_back = signal(0);

// ============================================================================
// Derived values
// ============================================================================

const trip_text = computed(() =>
  step_history.value.trip > TRIP_LIMIT ? `${TRIP_LIMIT}+` : `${step_history.value.trip}`,
);

const is_goal_met = computed(() => steps_today.value >= settings.step_goal.value);

/** The last seven days, oldest first, ending with today. */
const week_totals = computed(() => DAYS_SHOWN.map((days_back) => step_history.value.days[days_back].total));

const shown_hours = computed(() =>
  range(HOURS_PER_DAY).map((hour) => step_history.value.hours[shown_days_back.value * HOURS_PER_DAY + hour].steps),
);

/** The largest of the week's totals, which tops its chart; at least 1, so an empty week still draws. */
const week_most = computed(() => {
  let most = 1;
  for (const days_back of range(7)) {
    const total = step_history.value.days[days_back].total;
    if (total > most) most = total;
  }
  return most;
});

const shown_hour_most = computed(() => {
  let most = 1;
  for (const hour of range(24)) {
    const steps = step_history.value.hours[shown_days_back.value * HOURS_PER_DAY + hour].steps;
    if (steps > most) most = steps;
  }
  return most;
});

const shown_day_name = computed(() => {
  if (shown_days_back.value === 0) return "Today";
  if (shown_days_back.value === 1) return "Yesterday";
  return format_time(now.value - shown_days_back.value * DAY, "dddd");
});

// ============================================================================
// Handlers
// ============================================================================

function show_day(days_back: number) {
  shown_days_back.value = days_back;
  page.value = DAY_PAGE;
}

function turn_page(event: PageChangeEvent) {
  page.value = event.page;
}

function show_earlier_day() {
  shown_days_back.value = Math.min(shown_days_back.value + 1, STEP_HISTORY_DAYS - 1);
}

function show_later_day() {
  shown_days_back.value = Math.max(shown_days_back.value - 1, 0);
}

// ============================================================================
// Pages
// ============================================================================

function Today() {
  return (
    <Page align_items="center" padding={2}>
      <Arc
        diameter={236}
        thickness={10}
        value={steps_today.value}
        maximum={settings.step_goal.value}
        color={is_goal_met.value ? "success" : "accent"}
      >
        <Label font="large" color="#00ff00">{`${steps_today.value}`}</Label>
        <Label font="small" color="#b0b0b0">Steps</Label>
        <Label font="small" color="#b0b0b0">{`Yest: ${step_history.value.days[1].total}`}</Label>
        <Label font="small" color="#00ffff">{`Goal: ${settings.step_goal.value}`}</Label>
        <Label font="small" color="#ffff00">{`Trip: ${trip_text.value}`}</Label>
      </Arc>
      <Button x={60} y={188} width={120} height={46} radius={23} onClick={reset_step_trip}>
        Reset
      </Button>
    </Page>
  );
}

function DayButton(props: { days_back: number }) {
  return (
    <Button width={32} height={32} font="small" onClick={() => show_day(props.days_back)}>
      {format_time(now.value - props.days_back * DAY, "ddd")}
    </Button>
  );
}

function Week() {
  return (
    <Page padding={8} gap={6}>
      <Header title="Step history" icon={icons.steps} />
      <Label font="small" color="muted">Last 7 days; tap one</Label>
      <Chart width={224} height={96} values={week_totals.value} maximum={week_most.value} color="accent" />
      <Row justify="space_between" width={224}>
        {DAYS_SHOWN.map((days_back) => (
          <DayButton days_back={days_back} />
        ))}
      </Row>
    </Page>
  );
}

function Day() {
  return (
    <Page padding={8} gap={6}>
      <Header title={shown_day_name.value} icon={icons.steps} />
      <Row justify="space_between" align_items="center">
        <Button width={44} height={36} disabled={shown_days_back.value >= STEP_HISTORY_DAYS - 1} onClick={show_earlier_day}>
          {"<"}
        </Button>
        <Label font="large" color="success">{`${step_history.value.days[shown_days_back.value].total}`}</Label>
        <Button width={44} height={36} disabled={shown_days_back.value === 0} onClick={show_later_day}>
          {">"}
        </Button>
      </Row>
      <Column grow={1} justify="end" gap={2}>
        <Chart width={224} height={96} values={shown_hours.value} maximum={shown_hour_most.value} color="success" />
        <Row justify="space_between" width={224}>
          <Label font="small" color="muted">0</Label>
          <Label font="small" color="muted">6</Label>
          <Label font="small" color="muted">12</Label>
          <Label font="small" color="muted">18</Label>
          <Label font="small" color="muted">24</Label>
        </Row>
      </Column>
    </Page>
  );
}

// ============================================================================
// Screen
// ============================================================================

function Steps() {
  return (
    <Screen>
      <Pages grow={1} value={page.value} onPageChange={turn_page}>
        <Today />
        <Week />
        <Day />
      </Pages>
    </Screen>
  );
}

export default define_app({
  id: "steps",
  name: "Steps",
  group: "health",
  icon: icons.steps,
  screens: { main: Steps },
});
