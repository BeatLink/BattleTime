// System info, from InfiniTime's SystemInfo screen.
// One scrolling list of what the watch is running and how it is doing: the firmware's version and
// commit, how long it has run since it last started and why it restarted, the battery and the free
// storage.

import { computed } from "@preact/signals";
import {
  battery_percent,
  define_app,
  firmware_version,
  format_duration,
  free_storage_kilobytes,
  is_charging,
  now,
  reset_reason,
  started_at,
  DAY,
} from "neotime";
import { Header } from "neotime/components";
import { icons } from "neotime/icons";
import { Column, Label, Row, Screen, Scroll } from "neotime/widgets";

// ============================================================================
// Derived values
// ============================================================================

const KILOBYTES_IN_A_MEGABYTE = 1024;

const uptime = computed(() => now.value - started_at.value);

/** Whole days, then the hours, minutes and seconds of the day under way, as InfiniTime shows it. */
const uptime_text = computed(() => {
  const days = Math.floor(uptime.value / DAY);
  return `${days}d ${format_duration(uptime.value - days * DAY, "hh:mm:ss")}`;
});

const battery_text = computed(() => `${battery_percent.value}%${is_charging.value ? " charging" : ""}`);

/** Megabytes with one decimal place once there is a megabyte, and kilobytes below that. */
const free_storage_text = computed(() => {
  const kilobytes = free_storage_kilobytes.value;
  if (kilobytes < KILOBYTES_IN_A_MEGABYTE) return `${kilobytes} KB`;
  const megabytes = Math.trunc(kilobytes / KILOBYTES_IN_A_MEGABYTE);
  const tenths = Math.trunc(((kilobytes % KILOBYTES_IN_A_MEGABYTE) * 10) / KILOBYTES_IN_A_MEGABYTE);
  return `${megabytes}.${tenths} MB`;
});

// ============================================================================
// Screen
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

function SystemInfo() {
  return (
    <Screen padding={8} gap={4}>
      <Header title="System info" icon={icons.info} />
      <Scroll grow={1} gap={2}>
        <LongFact name="Firmware" value={firmware_version.value} />
        <Fact name="Uptime" value={uptime_text.value} />
        <LongFact name="Last restart" value={reset_reason.value} />
        <Fact name="Battery" value={battery_text.value} />
        <Fact name="Storage" value={`${free_storage_text.value} free`} />
      </Scroll>
    </Screen>
  );
}

export default define_app({
  id: "system-info",
  name: "System info",
  icon: icons.info,
  screens: { main: SystemInfo },
});
