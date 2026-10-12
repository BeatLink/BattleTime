// Common functions kept in the shared library as bytecode, so apps call them instead of copying them.
// Numbers on the watch are whole, so angles come back in thousandths.

import {
  current_weather,
  day_of_year,
  hour_of,
  minute_of,
  settings,
  steps_today,
  weekday_of,
  type Instant,
} from "./index.ts";

// ============================================================================
// Angles
// ============================================================================

/** The sine of an angle in degrees, in thousandths, read from a table on the watch. */
export function sine(degrees: number): number {
  return Math.round(Math.sin((degrees * Math.PI) / 180) * 1000);
}

/** The cosine of an angle in degrees, in thousandths. */
export function cosine(degrees: number): number {
  return Math.round(Math.cos((degrees * Math.PI) / 180) * 1000);
}

/** The point at a distance from a centre, at an angle in degrees clockwise from twelve o'clock. */
export function point_on_circle(
  center_x: number,
  center_y: number,
  radius: number,
  degrees: number,
): { x: number; y: number } {
  return {
    x: center_x + Math.round((radius * sine(degrees)) / 1000),
    y: center_y - Math.round((radius * cosine(degrees)) / 1000),
  };
}

// ============================================================================
// Functions the library holds as bytecode
// ============================================================================
// Apps call these by number instead of carrying their own copies. They read the system's values but
// never an app's state, so any app can share them. Their numbers are fixed by LIBRARY_FUNCTIONS in
// compiler/src/shared_functions.rs: new functions go at its end.

/** The hour a clock shows: 0 to 23 on a 24 hour clock, 1 to 12 on a 12 hour one. */
export function display_hour(instant: Instant): number {
  const hour = hour_of(instant);
  return settings.uses_24_hour_clock.value ? hour : hour % 12 === 0 ? 12 : hour % 12;
}

/** How many minutes into its local day an instant is, from 0 to 1439. */
export function minute_of_day(instant: Instant): number {
  return hour_of(instant) * 60 + minute_of(instant);
}

/** Whether an instant is between sunset and sunrise, or from 18:00 to 06:00 until the phone sends them. */
export function is_night(instant: Instant): boolean {
  const sunrise = current_weather.value.sunrise;
  const sunset = current_weather.value.sunset;
  if (sunrise === null || sunset === null) {
    return hour_of(instant) < 6 || hour_of(instant) >= 18;
  }
  const minute = hour_of(instant) * 60 + minute_of(instant);
  return minute < hour_of(sunrise) * 60 + minute_of(sunrise) || minute >= hour_of(sunset) * 60 + minute_of(sunset);
}

/** The steps taken since `base` steps today, starting again from today's count after midnight's reset. */
export function steps_since(base: number): number {
  return steps_today.value >= base ? steps_today.value - base : steps_today.value;
}

/** A temperature in whole degrees of the wearer's units, from degrees Celsius. */
export function temperature_in_unit(celsius: number): number {
  return settings.uses_metric_units.value ? celsius : Math.floor((celsius * 18 + 5) / 10) + 32;
}

/** One decimal digit of a number: `place` 1 for the units, 10 for the tens, and so on. */
export function digit_of(value: number, place: number): number {
  return Math.floor(value / place) % 10;
}

/** The week counted from the first Sunday of the year, as strftime's `%U` gives, so the days before it are week 0. */
export function sunday_week_of(instant: Instant): number {
  return Math.floor((day_of_year(instant) + 6 - ((weekday_of(instant) + 1) % 7)) / 7);
}

/** How many days a year has: 366 in a leap year, else 365. */
export function days_in_year(year: number): number {
  return (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0 ? 366 : 365;
}
