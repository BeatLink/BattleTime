// Lite photo watch face, ported from PinetimeLite's WatchFaceCustom and its three backgrounds.
// The time sits near the top with a colon that blinks each second, and the full date sits at the bottom.
// PinetimeLite draws it over a photo; until faces can show pictures, each backdrop is a plain colour and a long press picks the next.

import {
  computed,
  define_app,
  format_number,
  format_time,
  hour_of,
  minute_of,
  now,
  persisted,
  second_of,
  settings,
  type Color,
} from "battletime";
import { icons } from "battletime/icons";
import { Label, Row, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

/** PinetimeLite's night sky, coloured pencils and sunset photos, by their main colour. */
const BACKGROUNDS: Color[] = ["#1a1f3a", "#8a1c10", "#f08a4b"];

/** The time's colour over each backdrop. */
const TIME_COLORS: Color[] = ["#ffffff", "#ffffff", "#000000"];

/** The date's colour over each backdrop. */
const DATE_COLORS: Color[] = ["#ffffff", "#ffffff", "#ffff00"];

const BACKDROP_COUNT = 3;

const backdrop_index = persisted("backdrop_index", 0);

// ============================================================================
// Derived values
// ============================================================================

const backdrop_number = computed(() => backdrop_index.value % BACKDROP_COUNT);

const background_color = computed(() => BACKGROUNDS[backdrop_number.value]);

const time_color = computed(() => TIME_COLORS[backdrop_number.value]);

const date_color = computed(() => DATE_COLORS[backdrop_number.value]);

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

const hour_text = computed(() => {
  const hour = hour_of(now.value);
  if (uses_24_hour_clock.value) return format_number(hour, 2);
  return `${hour % 12 === 0 ? 12 : hour % 12}`;
});

const separator = computed(() => (second_of(now.value) % 2 === 0 ? ":" : " "));

/** A 12 hour clock writes the month before the day, as PinetimeLite does. */
const date_text = computed(() => format_time(now.value, uses_24_hour_clock.value ? "dddd, DD MMMM" : "dddd, MMMM DD"));

// ============================================================================
// Handlers
// ============================================================================

function next_backdrop() {
  backdrop_index.value = (backdrop_index.value + 1) % BACKDROP_COUNT;
}

// ============================================================================
// Screen
// ============================================================================

function LitePhotoFace() {
  return (
    <Screen background={background_color.value} onLongPress={next_backdrop}>
      <Row x={0} y={20} width={240} justify="center" gap={4}>
        <Label font="huge" color={time_color.value}>{hour_text.value}</Label>
        <Label font="huge" color={time_color.value}>{separator.value}</Label>
        <Label font="huge" color={time_color.value}>{format_number(minute_of(now.value), 2)}</Label>
      </Row>
      {!uses_24_hour_clock.value && (
        <Label x={0} y={104} width={240} text_align="center" font="small" color={time_color.value}>
          {hour_of(now.value) >= 12 ? "PM" : "AM"}
        </Label>
      )}
      <Label x={0} y={210} width={240} text_align="center" color={date_color.value}>
        {date_text.value}
      </Label>
    </Screen>
  );
}

export default define_app({
  id: "lite-photo",
  name: "Lite Photo",
  icon: icons.clock,
  kind: "face",
  screens: { main: LitePhotoFace },
});
