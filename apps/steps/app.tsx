// Step counter, merged from InfiniTime's Steps and wasp-os's step_counter and steplogger.
// The main screen is InfiniTime's goal ring with yesterday's count and a resettable trip meter,
// turning green once the goal is met as wasp-os's bar does; swipe up for wasp-os's history graphs.

import { computed } from "@preact/signals";
import {
  back,
  clock_change,
  define_app,
  format_time,
  go,
  hour_of,
  now,
  persisted,
  range,
  settings,
  steps,
  steps_today,
  DAY,
  type StepsEvent,
  type SwipeEvent,
} from "neotime";
import { Header } from "neotime/components";
import { icons } from "neotime/icons";
import { Arc, Button, Chart, Column, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const TRIP_LIMIT = 99999;

/** Days back from today that the history shows, oldest first. */
const DAYS_SHOWN = [6, 5, 4, 3, 2, 1, 0];

/** The day's count when the last step event was recorded. */
const recorded_steps = persisted("recorded_steps", 0);

/** Steps taken in each hour of today, recorded as they arrive. */
const hourly_steps = persisted<number[]>(
  "hourly_steps",
  [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
);

/** Totals of earlier days, yesterday first. */
const daily_totals = persisted<number[]>("daily_totals", [0, 0, 0, 0, 0, 0]);

/** Steps counted on the trip meter up to the last recorded event. */
const trip_steps = persisted("trip_steps", 0);

// ============================================================================
// Derived values
// ============================================================================

/** Steps taken since the last recorded event; a lower count means the day's count started again. */
const unrecorded_steps = computed(() =>
  steps_today.value >= recorded_steps.value ? steps_today.value - recorded_steps.value : steps_today.value,
);

const trip = computed(() => trip_steps.value + unrecorded_steps.value);

const trip_text = computed(() => (trip.value > TRIP_LIMIT ? `${TRIP_LIMIT}+` : `${trip.value}`));

const is_goal_met = computed(() => steps_today.value >= settings.step_goal.value);

const hourly_with_unrecorded = computed(() => {
  const current_hour = hour_of(now.value);
  return hourly_steps.value.map((count, hour) => (hour === current_hour ? count + unrecorded_steps.value : count));
});

/** The last seven days, oldest first, ending with today. */
const week_totals = computed(() => [
  daily_totals.value[5],
  daily_totals.value[4],
  daily_totals.value[3],
  daily_totals.value[2],
  daily_totals.value[1],
  daily_totals.value[0],
  steps_today.value,
]);

// ============================================================================
// Handlers
// ============================================================================

function record_steps(event: StepsEvent) {
  const new_steps =
    event.steps_today >= recorded_steps.value ? event.steps_today - recorded_steps.value : event.steps_today;
  const current_hour = hour_of(now.value);
  hourly_steps.value[current_hour] = hourly_steps.value[current_hour] + new_steps;
  trip_steps.value = trip_steps.value + new_steps;
  recorded_steps.value = event.steps_today;
}

/** The day's count may or may not have started again yet, and recording from it works either way. */
function start_new_day() {
  let total = unrecorded_steps.value;
  for (const hour of range(24)) {
    total = total + hourly_steps.value[hour];
    hourly_steps.value[hour] = 0;
  }
  daily_totals.value = [
    total,
    daily_totals.value[0],
    daily_totals.value[1],
    daily_totals.value[2],
    daily_totals.value[3],
    daily_totals.value[4],
  ];
  trip_steps.value = trip_steps.value + unrecorded_steps.value;
  recorded_steps.value = steps_today.value;
}

function reset_trip() {
  trip_steps.value = -unrecorded_steps.value;
}

function swipe_on_today(event: SwipeEvent) {
  if (event.direction === "up") go("history");
  else if (event.direction === "right") back();
}

function swipe_on_history(event: SwipeEvent) {
  if (event.direction === "down" || event.direction === "right") back();
}

// ============================================================================
// Screens
// ============================================================================

function Today() {
  return (
    <Screen align_items="center" padding={2} onSwipe={swipe_on_today}>
      <Arc
        diameter={236}
        thickness={10}
        value={steps_today.value}
        maximum={settings.step_goal.value}
        color={is_goal_met.value ? "success" : "accent"}
      >
        <Label font="large" color="#00ff00">{`${steps_today.value}`}</Label>
        <Label font="small" color="#b0b0b0">Steps</Label>
        <Label font="small" color="#b0b0b0">{`Yest: ${daily_totals.value[0]}`}</Label>
        <Label font="small" color="#00ffff">{`Goal: ${settings.step_goal.value}`}</Label>
        <Label font="small" color="#ffff00">{`Trip: ${trip_text.value}`}</Label>
      </Arc>
      <Button x={60} y={188} width={120} height={50} radius={25} onClick={reset_trip}>
        Reset
      </Button>
    </Screen>
  );
}

function History() {
  return (
    <Screen padding={6} gap={4} onSwipe={swipe_on_history}>
      <Header title="Step history" icon={icons.steps} />
      <Label font="small" color="muted">Last 7 days</Label>
      <Chart kind="bars" width={228} height={64} values={week_totals.value} color="accent" />
      <Row justify="space_between" width={228}>
        {DAYS_SHOWN.map((days_back) => (
          <Label font="small" color="muted" width={32} text_align="center">
            {format_time(now.value - days_back * DAY, "ddd")}
          </Label>
        ))}
      </Row>
      <Column grow={1} justify="end" gap={2}>
        <Label font="small" color="muted">Today by hour</Label>
        <Chart kind="bars" width={228} height={52} values={hourly_with_unrecorded.value} color="success" />
      </Column>
    </Screen>
  );
}

export default define_app({
  id: "steps",
  name: "Steps",
  icon: icons.steps,
  screens: { main: Today, history: History },
  subscriptions: [steps(record_steps), clock_change("day", start_new_day)],
  permissions: ["background"],
});
