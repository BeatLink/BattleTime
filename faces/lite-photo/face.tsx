// Lite photo watch face, ported from PinetimeLite's WatchFaceCustom and its three backgrounds.
// The time sits near the top with a colon that blinks each second, and the full date sits at the bottom.
// PinetimeLite draws it over a photo; until faces can show pictures, each backdrop is a plain colour and a long press picks the next.

import { computed } from "@preact/signals";
import { define_app, format_number, format_time, hour_of, minute_of, now, persisted, second_of, settings, type Color } from "neotime";
import { icons } from "neotime/icons";
import { Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

interface Backdrop {
  /** Stands in for the photo until faces can show pictures. */
  background: Color;
  time_color: Color;
  date_color: Color;
}

/** PinetimeLite's night sky, coloured pencils and sunset photos, by their main colour. */
const BACKDROPS: Backdrop[] = [
  { background: "#1a1f3a", time_color: "#ffffff", date_color: "#ffffff" },
  { background: "#8a1c10", time_color: "#ffffff", date_color: "#ffffff" },
  { background: "#f08a4b", time_color: "#000000", date_color: "#ffff00" },
];

const backdrop_index = persisted("backdrop_index", 0);

// ============================================================================
// Derived values
// ============================================================================

const backdrop = computed(() => BACKDROPS[backdrop_index.value % BACKDROPS.length]);

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
  backdrop_index.value = (backdrop_index.value + 1) % BACKDROPS.length;
}

// ============================================================================
// Screen
// ============================================================================

function LitePhotoFace() {
  const colors = backdrop.value;
  return (
    <Screen background={colors.background} onLongPress={next_backdrop}>
      <Row x={0} y={20} width={240} justify="center" gap={4}>
        <Label font="huge" color={colors.time_color}>{hour_text.value}</Label>
        <Label font="huge" color={colors.time_color}>{separator.value}</Label>
        <Label font="huge" color={colors.time_color}>{format_number(minute_of(now.value), 2)}</Label>
      </Row>
      {!uses_24_hour_clock.value && (
        <Label x={0} y={104} width={240} text_align="center" font="small" color={colors.time_color}>
          {hour_of(now.value) >= 12 ? "PM" : "AM"}
        </Label>
      )}
      <Label x={0} y={210} width={240} text_align="center" color={colors.date_color}>
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
