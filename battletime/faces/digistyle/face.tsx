// Digistyle watch face, ported from ZephyrLabs' Digistyle (ZephyrLabs/Watchfaces).
// An old digital watch: a dark bezel lettered PINETIME and WATER WR RESIST around a pale LCD panel with big digits and the date.
// The original's bezel is a full-screen picture; here it is redrawn with lines and boxes, and its seven-segment font is the huge font.

import {
  computed,
  define_app,
  format_number,
  format_time,
  heart_rate,
  hour_of,
  is_heart_rate_running,
  minute_of,
  now,
  settings,
  to_upper,
  unread_notifications,
  type Color,
} from "battletime";
import { display_hour } from "battletime/lib";
import { BatteryIcon, StepsBadge } from "battletime/components";
import { icons } from "battletime/icons";
import { Badge, Column, Icon, Label, Row, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

/** The bezel's lettering and lines, the grey of the original picture. */
const BEZEL_COLOR: Color = "#808080";

/** A pale LCD grey, close to how the original's panel looks on the watch. */
const PANEL_COLOR: Color = "#b4bcbc";

const INK_COLOR: Color = "#000000";

// ============================================================================
// Derived values
// ============================================================================

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

/** A 12 hour clock leaves the hour's leading zero blank, as an LCD would. */
const hour_text = computed(() => {
  const hour = display_hour(now.value);
  return uses_24_hour_clock.value ? format_number(hour, 2) : hour < 10 ? ` ${hour}` : `${hour}`;
});

const date_text = computed(() =>
  to_upper(format_time(now.value, uses_24_hour_clock.value ? "ddd D MMM YYYY" : "ddd MMM D YYYY")),
);

const heart_rate_text = computed(() => (is_heart_rate_running.value ? `${heart_rate.value}` : "---"));

// ============================================================================
// Screen
// ============================================================================

/** The black case with its lettering, rule lines and the WR badge. */
function Bezel() {
  return (
    <>
      <Label x={0} y={2} width={240} text_align="center" color={BEZEL_COLOR}>
        PINETIME
      </Label>
      <Column x={3} y={24} width={234} height={1} background={BEZEL_COLOR} />
      <Column x={3} y={27} width={234} height={172} radius={16} border_width={1} border_color={BEZEL_COLOR} />

      <Column x={2} y={204} width={236} height={1} background={BEZEL_COLOR} />
      <Label x={52} y={204} font="small" color={BEZEL_COLOR}>WATER</Label>
      <Column
        x={106}
        y={203}
        width={28}
        height={18}
        radius={5}
        border_width={1}
        border_color={BEZEL_COLOR}
        background="#000000"
        align_items="center"
        justify="center"
      >
        <Label font="small" color={BEZEL_COLOR}>WR</Label>
      </Column>
      <Label x={140} y={204} font="small" color={BEZEL_COLOR}>RESIST</Label>
    </>
  );
}

/** The pale LCD with the status, time and date in black. */
function Panel() {
  return (
    <Column x={7} y={30} width={226} height={166} radius={12} background={PANEL_COLOR}>
      {unread_notifications.value > 0 && <Icon x={8} y={2} icon={icons.notification} size={20} color={INK_COLOR} />}
      <Row x={118} y={2} width={100} height={20} justify="end">
        <BatteryIcon color={INK_COLOR} />
      </Row>
      {!uses_24_hour_clock.value && (
        <Label x={176} y={24} font="small" color={INK_COLOR}>
          {hour_of(now.value) >= 12 ? "PM" : "AM"}
        </Label>
      )}
      <Row x={0} y={58} width={226} justify="center">
        <Label font="huge" color={INK_COLOR}>{hour_text.value}</Label>
        <Label font="huge" color={INK_COLOR}>:</Label>
        <Label font="huge" color={INK_COLOR}>{format_number(minute_of(now.value), 2)}</Label>
      </Row>
      <Label x={0} y={138} width={226} text_align="center" color={INK_COLOR}>
        {date_text.value}
      </Label>
    </Column>
  );
}

function DigistyleFace() {
  return (
    <Screen>
      <Bezel />
      <Panel />
      <Badge x={5} y={221} height={18} icon={icons.heart} icon_size={18} gap={5} color={BEZEL_COLOR}>
        {heart_rate_text.value}
      </Badge>
      <StepsBadge x={120} y={221} width={115} height={18} align="end" icon_size={18} color={BEZEL_COLOR} />
    </Screen>
  );
}

export default define_app({
  id: "digistyle",
  name: "Digistyle",
  icon: icons.clock,
  kind: "face",
  screens: { main: DigistyleFace },
});
