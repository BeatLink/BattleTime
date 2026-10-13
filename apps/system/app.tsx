// System: the battery and how the watch is doing, as two pages swiped up and down.
// The battery page merges InfiniTime's BatteryInfo with PinetimeLite's charging and low battery
// screens: a ring with the charge that fills up again and again while charging, whether it is
// charging, full, low or critical in its colour, the voltage and a warning to charge below 10 %. The shell opens it
// when the charger is plugged in or out, or the charge goes low.
// The system page is InfiniTime's SystemInfo: the firmware, uptime, last restart and free storage.
// A scroll bar down the right edge, as wasp-os draws it, shows which page is on show.

import { computed } from "@preact/signals";
import {
  back,
  battery_millivolts,
  battery_percent,
  define_app,
  firmware_version,
  format_duration,
  format_fixed,
  free_storage_kilobytes,
  go,
  is_charging,
  now,
  reset_reason,
  started_at,
  DAY,
  SECOND,
  type Color,
  type SwipeEvent,
} from "neotime";
import { Header, ScrollBar } from "neotime/components";
import { icons } from "neotime/icons";
import { Arc, Column, Icon, Label, Row, Screen, Scroll } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** Below this charge, off the charger, the screen warns; the shell lights it once at this point. */
const WARNING_PERCENT = 10;
const LOW_PERCENT = 15;
const CRITICAL_PERCENT = 5;

/** While charging the ring fills from the charge to full in this many one-second steps, then starts again. */
const FILL_STEPS = 4;

const PAGES = 2;
const BATTERY_PAGE = 0;
const SYSTEM_PAGE = 1;

const KILOBYTES_IN_A_MEGABYTE = 1024;

// ============================================================================
// Battery values
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

const voltage_text = computed(() => `${format_fixed(Math.floor(battery_millivolts.value / 10), 2)} volts`);

/** While charging the ring fills again and again from the charge to full, one step a second; it stops with the screen. */
const ring_percent = computed(() =>
  is_charging.value
    ? battery_percent.value +
      Math.floor(((100 - battery_percent.value) * (Math.floor(now.value / SECOND) % (FILL_STEPS + 1))) / FILL_STEPS)
    : battery_percent.value,
);

// ============================================================================
// System values
// ============================================================================

const uptime = computed(() => now.value - started_at.value);

/** Whole days, then the hours, minutes and seconds of the day under way, as InfiniTime shows it. */
const uptime_text = computed(() => {
  const days = Math.floor(uptime.value / DAY);
  return `${days}d ${format_duration(uptime.value - days * DAY, "hh:mm:ss")}`;
});

/** Megabytes with one decimal place once there is a megabyte, and kilobytes below that. */
const free_storage_text = computed(() => {
  const kilobytes = free_storage_kilobytes.value;
  if (kilobytes < KILOBYTES_IN_A_MEGABYTE) return `${kilobytes} KB`;
  const megabytes = Math.trunc(kilobytes / KILOBYTES_IN_A_MEGABYTE);
  const tenths = Math.trunc(((kilobytes % KILOBYTES_IN_A_MEGABYTE) * 10) / KILOBYTES_IN_A_MEGABYTE);
  return `${megabytes}.${tenths} MB`;
});

// ============================================================================
// Handlers
// ============================================================================

function swipe_on_battery(event: SwipeEvent) {
  if (event.direction === "up") go("system");
  else if (event.direction === "right") back();
}

function swipe_on_system(event: SwipeEvent) {
  if (event.direction === "down" || event.direction === "right") back();
}

// ============================================================================
// Battery page
// ============================================================================

function BatteryPage() {
  return (
    <Screen onSwipe={swipe_on_battery}>
      <Column grow={1} align_items="center" justify="center" padding={4} gap={8}>
        <Arc diameter={168} thickness={12} value={ring_percent.value} color={status_color.value}>
          <Icon icon={is_charging.value ? icons.charging : icons.battery} size={24} color={status_color.value} />
          <Label font="large">{`${battery_percent.value}%`}</Label>
          <Label font="small" color={status_color.value}>
            {status_text.value}
          </Label>
        </Arc>
        <Label font="small" color={needs_charging.value ? "danger" : "warning"}>
          {needs_charging.value ? "Charge the watch" : voltage_text.value}
        </Label>
      </Column>
      <ScrollBar page={BATTERY_PAGE} pages={PAGES} />
    </Screen>
  );
}

// ============================================================================
// System page
// ============================================================================

/** A fact whose value may be long, on its own line under its name. */
function LongFact(props: { name: string; value: string }) {
  return (
    <Column padding={4} gap={0}>
      <Label font="small" color="muted">{props.name}</Label>
      <Label font="small">{props.value}</Label>
    </Column>
  );
}

/** One fact: its name on the left and its value on the right. */
function Fact(props: { name: string; value: string }) {
  return (
    <Row justify="space_between" height={24} padding={4}>
      <Label font="small" color="muted">{props.name}</Label>
      <Label font="small">{props.value}</Label>
    </Row>
  );
}

function SystemPage() {
  return (
    <Screen onSwipe={swipe_on_system}>
      <Column grow={1} padding={8} gap={4}>
        <Header title="System" icon={icons.info} />
        <Scroll grow={1} gap={2}>
          <LongFact name="Firmware" value={firmware_version.value} />
          <Fact name="Uptime" value={uptime_text.value} />
          <LongFact name="Last restart" value={reset_reason.value} />
          <Fact name="Storage" value={`${free_storage_text.value} free`} />
        </Scroll>
      </Column>
      <ScrollBar page={SYSTEM_PAGE} pages={PAGES} />
    </Screen>
  );
}

export default define_app({
  id: "system",
  name: "System",
  group: "system",
  icon: icons.info,
  screens: { main: BatteryPage, system: SystemPage },
});
