// Lite digital watch face, ported from PinetimeLite's WatchFaceDigital.
// The weekday and day sit over the month and year, above a white band with the time.
// A blue bar under the band grows with the seconds, which are written at its right end.
// Steps, Bluetooth, notifications and battery sit at the bottom.

import { computed } from "@preact/signals";
import {
  battery_percent,
  day_of,
  define_app,
  format_number,
  format_time,
  hour_of,
  is_charging,
  is_phone_connected,
  minute_of,
  now,
  second_of,
  settings,
  steps_today,
  unread_notifications,
  year_of,
  type Color,
} from "neotime";
import { icons } from "neotime/icons";
import { Column, Icon, Label, Line, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const WEEKDAY_COLOR: Color = "#ffe600";
const DAY_COLOR: Color = "#ce1b1b";
const MONTH_COLOR: Color = "#3077b9";
const YEAR_COLOR: Color = "#444444";
const SECONDS_COLOR: Color = "#888888";
const SECONDS_BAR_COLOR: Color = "#0094ff";
const STEPS_COLOR: Color = "#00ffe7";
const CONNECTED_COLOR: Color = "#0000ff";
const UNREAD_COLOR: Color = "#097500";
const OFF_COLOR: Color = "#2b2b2b";

const BAND_TOP = 75;
const BAND_HEIGHT = 90;
const SECONDS_BAR_Y = 162;

// ============================================================================
// Derived values
// ============================================================================

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

const display_hour = computed(() => {
  const hour = hour_of(now.value);
  return uses_24_hour_clock.value ? hour : hour % 12 === 0 ? 12 : hour % 12;
});

const time_text = computed(() => `${format_number(display_hour.value, 2)}:${format_number(minute_of(now.value), 2)}`);

/** The bar's right end, from the left edge at 0 seconds to the right edge at 59. */
const seconds_bar_end = computed(() => Math.floor((240 * second_of(now.value)) / 59));

const battery_text = computed(() => `${battery_percent.value}%`);

// ============================================================================
// Screen
// ============================================================================

function BatteryLevel() {
  return (
    <Row width={26} height={14} border_width={2} border_color="text" radius={2} padding={1}>
      <Column width={Math.max(1, Math.floor((battery_percent.value * 20) / 100))} height={8} background="text" />
    </Row>
  );
}

function LiteDigitalFace() {
  return (
    <Screen>
      <Column x={0} y={0} width={120} height={70} align_items="center" padding={6}>
        <Label color={WEEKDAY_COLOR}>{format_time(now.value, "dddd")}</Label>
        <Label font="large" color={DAY_COLOR}>{format_number(day_of(now.value), 2)}</Label>
      </Column>
      <Column x={120} y={0} width={120} height={70} align_items="center" padding={6}>
        <Label color={MONTH_COLOR}>{format_time(now.value, "MMMM")}</Label>
        <Label font="large" color={YEAR_COLOR}>{`${year_of(now.value)}`}</Label>
      </Column>

      <Row x={0} y={BAND_TOP} width={240} height={BAND_HEIGHT} background="#ffffff" />
      <Label x={0} y={BAND_TOP + 4} width={240} text_align="center" font="huge" color="#000000">
        {time_text.value}
      </Label>
      <Label x={200} y={BAND_TOP + BAND_HEIGHT + 2} width={36} text_align="right" font="small" color={SECONDS_COLOR}>
        {format_number(second_of(now.value), 2)}
      </Label>
      {!uses_24_hour_clock.value && (
        <Label x={4} y={BAND_TOP + BAND_HEIGHT + 2} font="small" color={SECONDS_COLOR}>
          {hour_of(now.value) >= 12 ? "PM" : "AM"}
        </Label>
      )}
      <Line
        points={[{ x: 0, y: SECONDS_BAR_Y }, { x: seconds_bar_end.value, y: SECONDS_BAR_Y }]}
        thickness={5}
        color={SECONDS_BAR_COLOR}
        rounded={false}
      />

      <Column x={16} y={185} width={60} height={50} align_items="center" gap={2}>
        <Icon icon={icons.steps} size={20} color={STEPS_COLOR} />
        <Label color={STEPS_COLOR}>{`${steps_today.value}`}</Label>
      </Column>
      <Column x={90} y={185} width={60} height={50} align_items="center" gap={2}>
        <Icon icon={icons.bluetooth} size={20} color={is_phone_connected.value ? CONNECTED_COLOR : OFF_COLOR} />
        <Icon icon={icons.notification} size={20} color={unread_notifications.value > 0 ? UNREAD_COLOR : OFF_COLOR} />
      </Column>
      <Column x={164} y={185} width={60} height={50} align_items="center" gap={2}>
        {is_charging.value ? <Icon icon={icons.charging} size={20} /> : <BatteryLevel />}
        <Label>{battery_text.value}</Label>
      </Column>
    </Screen>
  );
}

export default define_app({
  id: "lite-digital",
  name: "Lite Digital",
  icon: icons.clock,
  kind: "face",
  screens: { main: LiteDigitalFace },
});
