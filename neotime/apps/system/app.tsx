// System: how the watch is doing, on pages side by side, swiped left and right, as InfiniTime's
// SystemInfo screens are: the firmware, the battery, memory, the bootloader, the hardware, the tasks
// and the licence.
// The battery page merges InfiniTime's BatteryInfo with PinetimeLite's charging and low battery
// screens: a tall battery on the left that fills up again and again while charging, and on the right
// the charge, whether it is charging, full, low or critical in its colour, the voltage and a warning
// to charge below 10 %. The shell opens straight onto it when the charger is plugged in or out, or the
// charge goes low.
// The system page also says whether the bootloader keeps the firmware, and while it is on trial
// offers Confirm and Roll back, as InfiniTime's FirmwareValidation screen does.
// The firmware gives chip IDs and addresses as numbers, and this app writes them in hex.
// The engine's page bar along the bottom shows which page is on show.

import {
  accelerometer_chip_id,
  allocation_failures,
  battery_millivolts,
  battery_percent,
  bluetooth_address,
  bootloader_build_date,
  bootloader_crc,
  bootloader_version,
  computed,
  confirm_firmware,
  define_app,
  firmware_build_date,
  firmware_state,
  firmware_version,
  flash_id,
  format_duration,
  format_fixed,
  free_heap_bytes,
  free_storage_kilobytes,
  is_charging,
  least_free_heap_bytes,
  now,
  range,
  reset_reason,
  roll_back_firmware,
  stack_overflows,
  started_at,
  pad_start,
  slice_text,
  tasks,
  text_length,
  touch_chip_id,
  touch_firmware_version,
  DAY,
  SECOND,
  type Color,
} from "neotime";
import { Header } from "neotime/components";
import { icons } from "neotime/icons";
import { Button, Column, Label, Page, Pages, Row, Screen } from "neotime/widgets";

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
const BYTES_IN_A_KILOBYTE = 1024;

/** The most tasks the task page lists, as many as the firmware keeps running, two to a label. */
const MOST_TASKS = 10;
const TASKS_PER_LABEL = 2;

/** Divisors that bring each byte of a number down to the lowest, highest byte first. */
const BYTE_2_DIVISOR = 65_536;
const BYTE_3_DIVISOR = 16_777_216;
const BYTE_4_DIVISOR = 4_294_967_296;
const BYTE_5_DIVISOR = 1_099_511_627_776;

const UPPER_HEX_DIGITS = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "A", "B", "C", "D", "E", "F"];
const LOWER_HEX_DIGITS = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f"];

/** The chip IDs the PineTime's touch panels, accelerometers and flash chip report. */
const CST816S_CHIP_ID = 0xb4;
const CST816T_CHIP_ID = 0xb5;
const CST816D_CHIP_ID = 0xb6;
const BMA421_CHIP_ID = 0x11;
const BMA425_CHIP_ID = 0x13;
const XT25F32B_FLASH_ID = 0x0b4016;

const NO_ANSWER = "No answer";

/** Characters of the small font, JetBrains Mono at 16 pixels, across a page padded by 8, clear of the scroll bar. */
const LINE_CHARACTERS = 21;

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

// ============================================================================
// Hex and chip names
// ============================================================================

/** One hex digit of `value`: the one `divisor` brings down to the lowest. */
function hex_digit(value: number, divisor: number): string {
  return UPPER_HEX_DIGITS[Math.floor(value / divisor) % 16];
}

/** Two hex digits for one byte of `value`: the one `divisor` brings down to the lowest. */
function hex_byte(value: number, divisor: number): string {
  return `${hex_digit(value, divisor * 16)}${hex_digit(value, divisor)}`;
}

/** Six bytes, highest first, joined by colons, such as `D1:2A:5C:7E:90:3B`. */
const address_text = computed(() => {
  const address = bluetooth_address.value;
  return `${hex_byte(address, BYTE_5_DIVISOR)}:${hex_byte(address, BYTE_4_DIVISOR)}:${hex_byte(address, BYTE_3_DIVISOR)}:${hex_byte(address, BYTE_2_DIVISOR)}:${hex_byte(address, 256)}:${hex_byte(address, 1)}`;
});

/** The flash chip's name when it is the PineTime's own, then its JEDEC ID's three bytes. */
const flash_text = computed(() => {
  const id = flash_id.value;
  if (id === 0) return NO_ANSWER;
  const bytes = `${hex_byte(id, BYTE_2_DIVISOR)}${hex_byte(id, 256)}${hex_byte(id, 1)}`;
  return id === XT25F32B_FLASH_ID ? `XT25F32B ${bytes}` : bytes;
});

/** The chip's name, then the firmware version it reports. */
const touch_text = computed(() => {
  const id = touch_chip_id.value;
  const name =
    id === CST816S_CHIP_ID ? "CST816S" : id === CST816T_CHIP_ID ? "CST816T" : id === CST816D_CHIP_ID ? "CST816D" : `Chip ${id}`;
  return id === 0 ? NO_ANSWER : `${name} v${touch_firmware_version.value}`;
});

const accelerometer_text = computed(() => {
  const id = accelerometer_chip_id.value;
  return id === 0 ? NO_ANSWER : id === BMA421_CHIP_ID ? "BMA421" : id === BMA425_CHIP_ID ? "BMA425" : `Chip ${id}`;
});

/** One lowercase hex digit of a CRC, as the status page writes it. */
function crc_digit(crc: number, divisor: number): string {
  return LOWER_HEX_DIGITS[Math.floor(crc / divisor) % 16];
}

/** The version and, below it, the build date the bootloader carries, or else the first hex digits of its CRC-32 as the status page shows them. */
const bootloader_text = computed(() => {
  if (text_length(bootloader_version.value) > 0) {
    return `${bootloader_version.value}\nBuilt ${bootloader_build_date.value}`;
  }
  const crc = bootloader_crc.value;
  if (crc === null) return "Unreadable";
  return `CRC ${crc_digit(crc, 268_435_456)}${crc_digit(crc, 16_777_216)}${crc_digit(crc, 1_048_576)}${crc_digit(crc, 65_536)}${crc_digit(crc, 4096)}${crc_digit(crc, 256)}${crc_digit(crc, 16)}`;
});

/** Kilobytes with one decimal place. */
function kilobytes_text(bytes: number): string {
  return `${format_fixed(Math.floor((bytes * 10) / BYTES_IN_A_KILOBYTE), 1)} KB`;
}

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

/** What the bootloader thinks of the running firmware, in the order `firmware_state` numbers it. */
const FIRMWARE_STATE_NAMES = ["Confirmed", "On trial", "Rolled back"];
const FIRMWARE_ON_TRIAL = 1;

/** Each of the two firmware buttons, side by side across the page. */
const FIRMWARE_BUTTON_WIDTH = 109;
const FIRMWARE_BUTTON_HEIGHT = 36;

const firmware_state_name = computed(() => FIRMWARE_STATE_NAMES[firmware_state.value]);

const firmware_on_trial = computed(() => firmware_state.value === FIRMWARE_ON_TRIAL);

/** A fact whose value may be long, on its own line under its name. */
function LongFact(props: { name: string; value: string }) {
  return (
    <>
      <Label font="small" color="muted">{props.name}</Label>
      <Label font="small">{props.value}</Label>
    </>
  );
}

/** A page's name, in the accent colour as a header shows it, without the header's icon. */
function Title(props: { text: string }) {
  return <Label color="accent">{props.text}</Label>;
}

/** One fact on one line: its name on the left and its value padded out to the right, at least a space apart, as one label to spare widgets. */
function Fact(props: { name: string; value: string }) {
  return (
    <Label font="small" long_mode="clip">
      {`${props.name} ${pad_start(props.value, LINE_CHARACTERS - 1 - text_length(props.name))}`}
    </Label>
  );
}

function SystemPage() {
  return (
    <Page scrolls padding={8} gap={2}>
      <Header title="System" icon={icons.info} />
      <LongFact name="Firmware" value={firmware_version.value} />
      <Fact name="Built" value={firmware_build_date.value} />
      <Fact name="State" value={firmware_state_name.value} />
      {firmware_on_trial.value && (
        <Label font="small" color="warning">
          Any restart rolls back
        </Label>
      )}
      {firmware_on_trial.value && (
        <Row gap={6}>
          <Button
            width={FIRMWARE_BUTTON_WIDTH}
            height={FIRMWARE_BUTTON_HEIGHT}
            font="small"
            background="accent"
            onClick={confirm_firmware}
          >
            Confirm
          </Button>
          <Button
            width={FIRMWARE_BUTTON_WIDTH}
            height={FIRMWARE_BUTTON_HEIGHT}
            font="small"
            background="danger"
            onClick={roll_back_firmware}
          >
            Roll back
          </Button>
        </Row>
      )}
      <Fact name="Uptime" value={uptime_text.value} />
      <LongFact name="Last restart" value={reset_reason.value} />
    </Page>
  );
}

// ============================================================================
// Memory, bootloader and hardware pages
// ============================================================================

function MemoryPage() {
  return (
    <Page scrolls padding={8} gap={2}>
      <Title text="Memory" />
      <Fact name="Storage free" value={free_storage_text.value} />
      <Fact name="Heap free" value={kilobytes_text(free_heap_bytes.value)} />
      <Fact name="Least free" value={kilobytes_text(least_free_heap_bytes.value)} />
      <Fact name="Failed allocs" value={`${allocation_failures.value}`} />
      <Fact name="Stack overflows" value={`${stack_overflows.value}`} />
    </Page>
  );
}

function BootloaderPage() {
  return (
    <Page scrolls padding={8} gap={2}>
      <Title text="Bootloader" />
      <LongFact name="Version" value={bootloader_text.value} />
    </Page>
  );
}

function HardwarePage() {
  return (
    <Page scrolls padding={8} gap={2}>
      <Title text="Hardware" />
      <LongFact name="Bluetooth address" value={address_text.value} />
      <Fact name="Flash" value={flash_text.value} />
      <Fact name="Touch" value={touch_text.value} />
      <Fact name="Accelerometer" value={accelerometer_text.value} />
    </Page>
  );
}

// ============================================================================
// Tasks page
// ============================================================================

/** Characters for a task's name, then for its runs and its share of the time, so the columns line up in the monospaced font. */
const TASK_NAME_WIDTH = 10;
const TASK_RUNS_WIDTH = 6;
const TASK_SHARE_WIDTH = LINE_CHARACTERS - TASK_NAME_WIDTH - TASK_RUNS_WIDTH;

/** A count in at most five characters until a hundred million: as it is, then in thousands. */
function compact_count(count: number): string {
  return count < 100_000 ? `${count}` : `${Math.floor(count / 1000)}k`;
}

/** How much of the time since the watch started a task has spent running, in tenths of a percent. */
function busy_share_text(busy_milliseconds: number): string {
  return `${format_fixed(Math.floor((busy_milliseconds * 1000) / Math.max(uptime.value, 1)), 1)}%`;
}

/** A name padded to `width` characters; the firmware's task names are all shorter. */
function column_text(text: string, width: number): string {
  return `${text}${pad_start("", width - text_length(text))}`;
}

/** One task as a line: its name, how often it has run and its share of the time; nothing past the last task. */
function task_line(place: number): string {
  if (place >= tasks.value.length) return "";
  const task = tasks.value[place];
  return `${column_text(task.name, TASK_NAME_WIDTH)}${pad_start(compact_count(task.runs), TASK_RUNS_WIDTH)}${pad_start(busy_share_text(task.busy_milliseconds), TASK_SHARE_WIDTH)}`;
}

/** A later task's line after a line break, or nothing past the last task. */
function next_task_line(place: number): string {
  return place < tasks.value.length ? `\n${task_line(place)}` : "";
}

/** Two tasks' lines from `first`, as one label: a label per task needs more heap than an app may have, and more lines fill the text buffer. */
function TaskLines(props: { first: number }) {
  return (
    <Label font="small">
      {`${task_line(props.first)}${next_task_line(props.first + 1)}`}
    </Label>
  );
}

function TasksPage() {
  return (
    <Page scrolls padding={8} gap={0}>
      <Title text="Tasks" />
      <Label font="small" color="muted">
        {`${column_text("Task", TASK_NAME_WIDTH)}${pad_start("Runs", TASK_RUNS_WIDTH)}${pad_start("CPU", TASK_SHARE_WIDTH)}`}
      </Label>
      {range(MOST_TASKS / TASKS_PER_LABEL).map((group) => (
        <TaskLines first={group * TASKS_PER_LABEL} />
      ))}
    </Page>
  );
}

// ============================================================================
// Licence page
// ============================================================================

function LicencePage() {
  return (
    <Page padding={8} gap={6}>
      <Title text="Licence" />
      <Label font="small">{"NeoTime is free software under the GNU GPL, version 3 or later.\n\nIt comes with no warranty."}</Label>
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
        <MemoryPage />
        <BootloaderPage />
        <HardwarePage />
        <TasksPage />
        <LicencePage />
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
