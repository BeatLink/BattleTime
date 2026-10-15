// Waves watch face, ported from ZephyrLabs' Waves (ZephyrLabs/Watchfaces).
// Big hours over big minutes with the date between them, each with a dark shadow, over a beach seen from above.
// Waves shows a photo of sea, surf and sand; here it is redrawn as bands of those colours, because the photo does not fit in a face's memory.

import {
  computed,
  day_of,
  define_app,
  format_number,
  heart_rate,
  is_charging,
  is_phone_connected,
  minute_of,
  month_of,
  now,
  start_of_minute,
  unread_notifications,
  weekday_of,
  year_of,
  type Color,
} from "battletime";
import { display_hour } from "battletime/lib";
import { BatteryIcon, StepsBadge } from "battletime/components";
import { icons } from "battletime/icons";
import { Badge, Column, Icon, Label, Row, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

const DEEP_SEA: Color = "#1b4d74";
const OPEN_SEA: Color = "#21698c";
const SHALLOW_SEA: Color = "#2d8ea4";
const TURQUOISE: Color = "#55bcc2";
const FOAM: Color = "#eef6f3";
const WET_SAND: Color = "#c2a97e";
const SAND: Color = "#dbc6a0";
const SHADOW: Color = "#404040";
const WHITE: Color = "#ffffff";

/** Waves writes the date in lower case, which the text functions cannot do, so the names are spelt out. */
const WEEKDAY_NAMES = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const MONTH_NAMES = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

// ============================================================================
// Derived values
// ============================================================================

/** The face shows no seconds, so it reads the time cut to the minute and wakes once a minute. */
const minute_now = computed(() => start_of_minute(now.value));

const hour_text = computed(() => format_number(display_hour(minute_now.value), 2));

const minute_text = computed(() => format_number(minute_of(minute_now.value), 2));

const date_text = computed(
  () =>
    `${WEEKDAY_NAMES[weekday_of(minute_now.value)]}. ${day_of(minute_now.value)} ${MONTH_NAMES[month_of(minute_now.value) - 1]} ${year_of(minute_now.value)}`,
);

// ============================================================================
// Screen
// ============================================================================

/** Deep water at the top, shallows, a ragged line of surf, then the sand. */
function Beach() {
  return (
    <>
      <Column x={0} y={0} width={240} height={64} background={DEEP_SEA} />
      <Column x={0} y={64} width={240} height={34} background={OPEN_SEA} />
      <Column x={0} y={98} width={240} height={22} background={SHALLOW_SEA} />
      <Column x={0} y={120} width={240} height={14} background={TURQUOISE} />
      <Column x={0} y={140} width={240} height={100} background={SAND} />
      <Column x={0} y={140} width={240} height={14} background={WET_SAND} />
      <Column x={-20} y={124} width={110} height={26} radius={13} background={FOAM} />
      <Column x={70} y={130} width={100} height={22} radius={11} background={FOAM} />
      <Column x={150} y={122} width={110} height={28} radius={14} background={FOAM} />
      <Column x={20} y={146} width={70} height={14} radius={7} background={FOAM} />
      <Column x={170} y={148} width={60} height={12} radius={6} background={FOAM} />
    </>
  );
}

/** White text over the same text in dark grey, nudged right and down, as Waves draws its shadows. */
function ShadowedLabel(props: { y: number; shift_x: number; shift_y: number; font: "normal" | "huge"; text: string }) {
  return (
    <>
      <Label x={props.shift_x} y={props.y + props.shift_y} width={240} text_align="center" font={props.font} color={SHADOW}>
        {props.text}
      </Label>
      <Label x={0} y={props.y} width={240} text_align="center" font={props.font} color={WHITE}>
        {props.text}
      </Label>
    </>
  );
}

function WavesFace() {
  return (
    <Screen>
      <Beach />

      {unread_notifications.value > 0 && <Icon x={4} y={2} icon={icons.notification} size={20} color="#00ff00" />}
      <Row x={150} y={2} width={86} height={22} justify="end" gap={6}>
        {is_phone_connected.value && <Icon icon={icons.bluetooth} size={18} color={WHITE} />}
        {is_charging.value ? <Icon icon={icons.charging} size={18} color={WHITE} /> : <BatteryIcon single_color color={WHITE} />}
      </Row>

      <ShadowedLabel y={22} shift_x={4} shift_y={2} font="huge" text={hour_text.value} />
      <ShadowedLabel y={102} shift_x={2} shift_y={1} font="normal" text={date_text.value} />
      <ShadowedLabel y={136} shift_x={4} shift_y={2} font="huge" text={minute_text.value} />

      <Badge x={4} y={216} height={22} icon={icons.heart} icon_size={20} gap={5} color={WHITE}>
        {`${heart_rate.value} BPM`}
      </Badge>
      <StepsBadge x={140} y={216} width={96} height={22} icon_size={20} align="end" color={WHITE} />
    </Screen>
  );
}

export default define_app({
  id: "waves",
  name: "Waves",
  icon: icons.clock,
  kind: "face",
  screens: { main: WavesFace },
});
