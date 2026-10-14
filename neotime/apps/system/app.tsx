// System: how the watch is doing and its battery, as two pages side by side, swiped left and right.
// The system page is InfiniTime's SystemInfo: the firmware, uptime, last restart and free storage.
// The battery page merges InfiniTime's BatteryInfo with PinetimeLite's charging and low battery
// screens: a tall battery on the left that fills up again and again while charging, and on the right
// the charge, whether it is charging, full, low or critical in its colour, the voltage and a warning
// to charge below 10 %. The shell opens straight onto it when the charger is plugged in or out, or the
// charge goes low.
// The engine's page bar along the bottom shows which page is on show.

import {
  battery_millivolts,
  battery_percent,
  computed,
  define_app,
  firmware_version,
  format_duration,
  format_fixed,
  free_storage_kilobytes,
  is_charging,
  now,
  reset_reason,
  started_at,
  DAY,
  SECOND,
  type Color,
} from "neotime";
import { Header } from "neotime/components";
import { icons } from "neotime/icons";
import { Column, Label, Page, Pages, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

/** Below this charge, off the charger, the screen warns; the shell lights it once at this point. */
const WARNING_PERCENT = 10;
const LOW_PERCENT = 15;
const CRITICAL_PERCENT = 5;

/** While charging the battery fills from the charge to full in this many one-second steps, then starts again. */
const FILL_STEPS = 4;

const SYSTEM_PAGE = 0;
const BATTERY_PAGE = 1;

/** The battery's size, its outline and the gap between outline and fill. */
const METER_WIDTH = 72;
const METER_HEIGHT = 176;
const METER_OUTLINE = 4;
const METER_GAP = 4;
const METER_FILL_HEIGHT = METER_HEIGHT - 2 * (METER_OUTLINE + METER_GAP);
const TERMINAL_WIDTH = 28;
const TERMINAL_HEIGHT = 10;

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
          : "Almost empty",
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

/** While charging the battery fills again and again from the charge to full, one step a second; it stops with the screen. */
const meter_percent = computed(() =>
  is_charging.value
    ? battery_percent.value +
      Math.floor(((100 - battery_percent.value) * (Math.floor(now.value / SECOND) % (FILL_STEPS + 1))) / FILL_STEPS)
    : battery_percent.value,
);

const meter_fill_height = computed(() => Math.round((METER_FILL_HEIGHT * meter_percent.value) / 100));

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
    <Page scrolls padding={8} gap={2}>
      <Header title="System" icon={icons.info} />
      <LongFact name="Firmware" value={firmware_version.value} />
      <Fact name="Uptime" value={uptime_text.value} />
      <LongFact name="Last restart" value={reset_reason.value} />
      <Fact name="Storage" value={`${free_storage_text.value} free`} />
    </Page>
  );
}

// ============================================================================
// Battery page
// ============================================================================

/** A tall battery, its terminal on top, filled from the bottom to the charge in the status colour. */
function BatteryMeter() {
  return (
    <Column align_items="center" gap={0}>
      <Column width={TERMINAL_WIDTH} height={TERMINAL_HEIGHT} radius={2} background="text" />
      <Column
        width={METER_WIDTH}
        height={METER_HEIGHT}
        border_width={METER_OUTLINE}
        border_color="text"
        radius={10}
        padding={METER_GAP}
        justify="end"
      >
        <Column height={meter_fill_height.value} radius={4} background={status_color.value} />
      </Column>
    </Column>
  );
}

function BatteryPage() {
  return (
    <Page>
      <Row grow={1} align_items="center" padding={12} gap={14}>
        <BatteryMeter />
        <Column grow={1} gap={6}>
          <Label font="large">{`${battery_percent.value}%`}</Label>
          <Label font="small" color={status_color.value}>
            {status_text.value}
          </Label>
          <Label font="small" color="muted">
            {voltage_text.value}
          </Label>
          {needs_charging.value && (
            <Label font="small" color="danger">
              Charge now
            </Label>
          )}
        </Column>
      </Row>
    </Page>
  );
}

// ============================================================================
// Screens
// ============================================================================

/** Both pages, opening on `start`. */
function SystemPages(props: { start: number }) {
  return (
    <Screen>
      <Pages grow={1} value={props.start}>
        <SystemPage />
        <BatteryPage />
      </Pages>
    </Screen>
  );
}

function SystemScreen() {
  return <SystemPages start={SYSTEM_PAGE} />;
}

/** The screen the shell opens for a battery event: the same pages, opening on the battery. */
function BatteryScreen() {
  return <SystemPages start={BATTERY_PAGE} />;
}

export default define_app({
  id: "system",
  name: "System",
  group: "system",
  icon: icons.info,
  screens: { main: SystemScreen, battery: BatteryScreen },
});
