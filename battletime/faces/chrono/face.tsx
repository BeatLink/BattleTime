// Chrono watch face, ported from ZephyrLabs' Chrono (ZephyrLabs/Watchfaces).
// A chronograph dial with a pine cone, two stopwatch subdials and a small globe, the "atlas", under the hands.
// A tap starts the stopwatch from zero or stops it, and a long press clears a stopped one, as Chrono's swipe and long tap did.
// The globe is drawn from rounded blocks, because Chrono's earth pictures do not fit in a face's memory.

import {
  computed,
  define_app,
  heart_rate,
  is_charging,
  is_phone_connected,
  now,
  persisted,
  unread_notifications,
  utc_offset,
  MINUTE,
  SECOND,
  type Color,
  type Duration,
  type Instant,
} from "battletime";
import { BatteryIcon, StepsBadge } from "battletime/components";
import { icons } from "battletime/icons";
import { AnalogClock, Badge, Column, Icon, Label, Row, Scale, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

const DIAL_GRAY: Color = "#cbcbcb";
const TICK_GRAY: Color = "#8e8e8e";
const HOUR_WHITE: Color = "#eeeeee";
const MINUTE_WHITE: Color = "#cccccc";
const SECOND_RED: Color = "#ed1c24";
const BLACK: Color = "#000000";
const SEA_BLUE: Color = "#2f6fd6";
const DEEP_BLUE: Color = "#1d4fa8";
const LAND_GREEN: Color = "#6f9a4a";
const DESERT_TAN: Color = "#c8ae7c";
const CLOUD_WHITE: Color = "#e8f0ff";

/** Chrono's hand lengths in pixels from the centre. */
const HOUR_LENGTH = 57;
const MINUTE_LENGTH = 77;
const SECOND_LENGTH = 97;
const SUBDIAL_HAND_LENGTH = 25;

/** The ticks under the numerals at 12, 3, 6 and 9, counted in minutes. */
const NUMERAL_TICKS = [0, 15, 30, 45];

/** When the stopwatch started, or null while it is stopped. */
const started_at = persisted<Instant | null>("started_at", null);

/** The time the stopwatch showed when it was stopped. */
const stopped_after = persisted<Duration>("stopped_after", 0);

// ============================================================================
// Derived values
// ============================================================================

const elapsed = computed(() => (started_at.value !== null ? now.value - started_at.value : stopped_after.value));

/** The clock draws local time, so the zone's offset is taken off to point a second hand at the stopwatch's seconds. */
const seconds_hand_time = computed(() => elapsed.value - utc_offset.value);

/** One stopwatch minute moves the left subdial's hand as far as one second moves the right one's. */
const minutes_hand_time = computed(() => Math.floor(elapsed.value / MINUTE) * SECOND - utc_offset.value);

// ============================================================================
// Handlers
// ============================================================================

/** Starts the stopwatch from zero, or stops it and keeps the time it reached. */
function start_or_stop() {
  if (started_at.value !== null) {
    stopped_after.value = now.value - started_at.value;
    started_at.value = null;
  } else {
    started_at.value = now.value;
  }
}

/** Clears a stopped stopwatch; a running one is left alone. */
function clear_stopwatch() {
  if (started_at.value === null) {
    stopped_after.value = 0;
  }
}

// ============================================================================
// Screen
// ============================================================================

/** Twelve marks round one stopwatch subdial, with a thin hand on the subdial's own clock. */
function Subdial(props: { x: number; time: Instant }) {
  return (
    <>
      <Scale x={props.x} y={89} diameter={62} ticks={12} major_every={3} color={TICK_GRAY} major_color={DIAL_GRAY} />
      <AnalogClock
        x={props.x}
        y={89}
        diameter={62}
        time={props.time}
        show_seconds
        hour_length={0}
        minute_length={0}
        hour_color={BLACK}
        minute_color={BLACK}
        second_length={SUBDIAL_HAND_LENGTH}
        second_color={HOUR_WHITE}
        hand_style="line"
      />
    </>
  );
}

/** The atlas: a blue globe with two continents and a cloud. */
function Globe() {
  return (
    <Column x={90} y={150} width={60} height={60} radius={30} background={SEA_BLUE} border_color={DEEP_BLUE} border_width={2}>
      <Column x={22} y={6} width={22} height={14} radius={7} background={LAND_GREEN} />
      <Column x={26} y={18} width={18} height={26} radius={9} background={DESERT_TAN} />
      <Column x={8} y={22} width={12} height={10} radius={5} background={LAND_GREEN} />
      <Column x={12} y={44} width={22} height={4} radius={2} background={CLOUD_WHITE} />
    </Column>
  );
}

function ChronoFace() {
  return (
    <Screen onClick={start_or_stop} onLongPress={clear_stopwatch}>
      <Scale
        x={6}
        y={6}
        diameter={228}
        ticks={60}
        major_every={5}
        skip={NUMERAL_TICKS}
        color={TICK_GRAY}
        major_color={DIAL_GRAY}
      />
      <Label x={100} y={11} width={40} text_align="center" color={DIAL_GRAY}>
        12
      </Label>
      <Label x={207} y={111} width={20} text_align="center" color={DIAL_GRAY}>
        3
      </Label>
      <Label x={110} y={210} width={20} text_align="center" color={DIAL_GRAY}>
        6
      </Label>
      <Label x={13} y={111} width={20} text_align="center" color={DIAL_GRAY}>
        9
      </Label>
      <Icon x={104} y={34} icon={icons.pine_logo} size={32} color={DIAL_GRAY} />

      <Subdial x={29} time={minutes_hand_time.value} />
      <Subdial x={149} time={seconds_hand_time.value} />
      <Globe />

      {unread_notifications.value > 0 && <Icon x={0} y={0} icon={icons.notification} size={20} color="#00ff00" />}
      <Row x={150} y={0} width={86} height={22} justify="end" gap={6}>
        {is_phone_connected.value && <Icon icon={icons.bluetooth} size={18} />}
        {is_charging.value ? <Icon icon={icons.charging} size={18} /> : <BatteryIcon single_color />}
      </Row>
      <Badge x={4} y={216} height={22} icon={icons.heart} icon_size={20} gap={5}>
        {`${heart_rate.value} BPM`}
      </Badge>
      <StepsBadge x={140} y={216} width={96} height={22} icon_size={20} align="end" />

      <AnalogClock
        time={now.value}
        show_seconds
        hour_length={HOUR_LENGTH}
        minute_length={MINUTE_LENGTH}
        second_length={SECOND_LENGTH}
        hand_style="rounded"
        hour_color={HOUR_WHITE}
        minute_color={MINUTE_WHITE}
        second_color={SECOND_RED}
      />
    </Screen>
  );
}

export default define_app({
  id: "chrono",
  name: "Chrono",
  icon: icons.stopwatch,
  kind: "face",
  screens: { main: ChronoFace },
});
