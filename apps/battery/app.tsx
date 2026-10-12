// Battery, merged from InfiniTime's BatteryInfo and PinetimeLite's charging and low battery screens.
// From InfiniTime: a ring with the charge, whether it is charging, full, low or critical in its
// colour, and the voltage. From PinetimeLite: a bar that fills while charging, and a warning to
// charge below 10 %. The shell opens it when the charger is plugged in or out, or the charge goes low.

import { computed } from "@preact/signals";
import {
  battery_millivolts,
  battery_percent,
  define_app,
  format_number,
  is_charging,
  now,
  SECOND,
  type Color,
} from "neotime";
import { icons } from "neotime/icons";
import { Arc, Bar, Icon, Label, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** Below this charge, off the charger, the screen warns; the shell lights it once at this point. */
const WARNING_PERCENT = 10;
const LOW_PERCENT = 15;
const CRITICAL_PERCENT = 5;

/** The charging bar fills from the charge to full in this many one-second steps, then starts again. */
const FILL_STEPS = 4;

// ============================================================================
// Derived values
// ============================================================================

const status_text = computed(() =>
  is_charging.value
    ? "Charging"
    : battery_percent.value >= 100
      ? "Fully charged"
      : battery_percent.value > LOW_PERCENT
        ? "Discharging"
        : battery_percent.value > CRITICAL_PERCENT
          ? "Battery low"
          : "Battery critical",
);

const status_color = computed<Color>(() =>
  is_charging.value
    ? "success"
    : battery_percent.value >= 100
      ? "accent"
      : battery_percent.value > LOW_PERCENT
        ? "success"
        : battery_percent.value > CRITICAL_PERCENT
          ? "warning"
          : "danger",
);

const needs_charging = computed(() => !is_charging.value && battery_percent.value < WARNING_PERCENT);

const voltage_text = computed(
  () =>
    `${Math.floor(battery_millivolts.value / 1000)}.${format_number(Math.floor(battery_millivolts.value / 10) % 100, 2)} volts`,
);

/** While charging the bar fills again and again from the charge to full, one step a second; it stops with the screen. */
const bar_percent = computed(() =>
  is_charging.value
    ? battery_percent.value +
      Math.floor(((100 - battery_percent.value) * (Math.floor(now.value / SECOND) % (FILL_STEPS + 1))) / FILL_STEPS)
    : battery_percent.value,
);

// ============================================================================
// Screen
// ============================================================================

function BatteryScreen() {
  return (
    <Screen align_items="center" padding={4} gap={4}>
      <Arc diameter={168} thickness={12} value={battery_percent.value} color={status_color.value}>
        <Icon icon={is_charging.value ? icons.charging : icons.battery} size={24} color={status_color.value} />
        <Label font="large">{`${battery_percent.value}%`}</Label>
        <Label font="small" color={status_color.value}>
          {status_text.value}
        </Label>
      </Arc>
      <Bar width={200} value={bar_percent.value} color={status_color.value} />
      <Label font="small" color={needs_charging.value ? "danger" : "warning"}>
        {needs_charging.value ? "Charge the watch" : voltage_text.value}
      </Label>
    </Screen>
  );
}

export default define_app({
  id: "battery",
  name: "Battery",
  icon: icons.battery,
  screens: { main: BatteryScreen },
});
