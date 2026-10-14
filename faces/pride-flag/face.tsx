// Pride flag watch face, ported from InfiniTime's WatchFacePrideFlag.
// The screen is the flag's stripes, with battery, date, time, weekday and steps spaced down the middle.
// InfiniTime's pop-up flag buttons are a settings screen here, opened by a long press.

import {
  back,
  battery_percent,
  computed,
  define_app,
  format_time,
  go,
  is_charging,
  is_phone_connected,
  now,
  persisted,
  settings,
  steps_today,
  unread_notifications,
  type Color,
} from "neotime";
import { icons } from "neotime/icons";
import { Button, Column, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const FLAG_NAMES = ["Gay", "Trans", "Bi", "Lesbian"];
const FLAG_COUNT = 4;
const MOST_STRIPES = 7;

/** How many stripes each flag has. */
const STRIPE_COUNTS = [7, 5, 5, 7];

/** Seven stripe colours for each flag, one flag after another; flags with fewer stripes leave the rest unused. */
const STRIPE_COLORS: Color[] = [
  "#078d70", "#26ceaa", "#98e8c1", "#ffffff", "#00bff3", "#5049cc", "#3d1a78",
  "#00bff3", "#f49ac1", "#ffffff", "#f49ac1", "#00bff3", "#000000", "#000000",
  "#d60270", "#d60270", "#9b4f96", "#0038a8", "#0038a8", "#000000", "#000000",
  "#ff0000", "#ef7627", "#ff9b55", "#ffffff", "#d461a6", "#b55690", "#a50062",
];

const TOP_LABEL_COLORS: Color[] = ["#000000", "#ffffff", "#000000", "#ffffff"];
const TIME_COLORS: Color[] = ["#000000", "#000000", "#ffffff", "#000000"];
const BOTTOM_LABEL_COLORS: Color[] = ["#ffffff", "#ffffff", "#000000", "#ffffff"];

const STRIPE_PLACES = [0, 1, 2, 3, 4, 5, 6];

const flag = persisted("flag", 0);

// ============================================================================
// Derived values
// ============================================================================

const stripe_count = computed(() => STRIPE_COUNTS[flag.value]);

/** The gap between the lines of text, which grows with the number of stripes as InfiniTime's does. */
const spacing = computed(() => Math.floor((3 * stripe_count.value + 81) / 2));

const time_text = computed(() =>
  format_time(now.value, settings.uses_24_hour_clock.value ? "HH:mm:ss" : "hh:mm:ss"),
);

/** The weekday's full name, with AM or PM after it on a 12 hour clock, as InfiniTime shows it. */
const day_text = computed(() => format_time(now.value, settings.uses_24_hour_clock.value ? "dddd" : "dddd A"));

// ============================================================================
// Handlers
// ============================================================================

function next_flag() {
  flag.value = (flag.value + 1) % FLAG_COUNT;
}

function previous_flag() {
  flag.value = flag.value > 0 ? flag.value - 1 : FLAG_COUNT - 1;
}

// ============================================================================
// Screens
// ============================================================================

function Stripes(props: { top: number; height: number }) {
  return (
    <>
      {STRIPE_PLACES.map((place) => (
        <>
          {place < stripe_count.value && (
            <Column
              x={0}
              y={props.top + Math.floor((place * props.height) / stripe_count.value)}
              width={240}
              height={Math.floor(props.height / stripe_count.value) + 1}
              background={STRIPE_COLORS[flag.value * MOST_STRIPES + place]}
            />
          )}
        </>
      ))}
    </>
  );
}

/** A line of text centred across the screen, `offset` lines of spacing above or below the middle. */
function CentredLine(props: { offset: number; color: Color; text: string }) {
  return (
    <Label x={0} y={108 + props.offset * spacing.value} width={240} text_align="center" color={props.color}>
      {props.text}
    </Label>
  );
}

function PrideFlagFace() {
  return (
    <Screen onLongPress={() => go("settings")}>
      <Stripes top={0} height={240} />

      {is_phone_connected.value && <Icon x={192} y={0} icon={icons.bluetooth} size={24} />}
      {unread_notifications.value > 0 && (
        <Label x={0} y={0} color="#000000">
          {"You have\nmail!"}
        </Label>
      )}

      <CentredLine
        offset={-2}
        color={TOP_LABEL_COLORS[flag.value]}
        text={is_charging.value ? `${battery_percent.value}% Charging` : `${battery_percent.value}%`}
      />
      <CentredLine offset={-1} color={TOP_LABEL_COLORS[flag.value]} text={format_time(now.value, "DD-MM-YYYY")} />
      <Label x={0} y={95} width={240} text_align="center" font="large" color={TIME_COLORS[flag.value]}>
        {time_text.value}
      </Label>
      <CentredLine offset={1} color={BOTTOM_LABEL_COLORS[flag.value]} text={day_text.value} />
      <CentredLine offset={2} color={BOTTOM_LABEL_COLORS[flag.value]} text={`${steps_today.value} steps`} />
    </Screen>
  );
}

function Settings() {
  return (
    <Screen>
      <Stripes top={40} height={110} />
      <Label x={0} y={8} width={240} text_align="center">
        {FLAG_NAMES[flag.value]}
      </Label>
      <Row x={8} y={168} width={224} height={64} gap={8}>
        <Button grow={1} height="100%" onClick={previous_flag}>
          {"<"}
        </Button>
        <Button grow={1} height="100%" background="accent" onClick={back}>
          <Icon icon={icons.check} size={28} />
        </Button>
        <Button grow={1} height="100%" onClick={next_flag}>
          {">"}
        </Button>
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "pride-flag",
  name: "Pride flag",
  icon: icons.flag,
  kind: "face",
  screens: { main: PrideFlagFace, settings: Settings },
});
