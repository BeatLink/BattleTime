// Park Skyline watch face, ported from jlukanc1's Park Skyline in ZephyrLabs' collection (ZephyrLabs/Watchfaces).
// The date and a big black time on a pale evening sky, over the black treeline of a park with the heart rate and steps.
// Park Skyline shows a photo; here the sky is bands of blue and the trees a row of black crowns, because the photo does not fit in a face's memory.

import {
  computed,
  define_app,
  format_number,
  format_time,
  heart_rate,
  is_charging,
  is_phone_connected,
  minute_of,
  now,
  start_of_minute,
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

const HIGH_SKY: Color = "#9dbfdc";
const MIDDLE_SKY: Color = "#b6d0e4";
const LOW_SKY: Color = "#cddfeb";
const HAZE: Color = "#e2ecf1";
const BLACK: Color = "#000000";
const WHITE: Color = "#ffffff";

/** Each tree's crown as a circle, left to right, written out because the compiler keeps only literal places. */
const TREE_CROWNS = [
  { x: -8, y: 170, diameter: 26 },
  { x: 14, y: 176, diameter: 20 },
  { x: 28, y: 164, diameter: 28 },
  { x: 52, y: 172, diameter: 22 },
  { x: 70, y: 178, diameter: 18 },
  { x: 84, y: 160, diameter: 30 },
  { x: 110, y: 170, diameter: 24 },
  { x: 130, y: 176, diameter: 20 },
  { x: 146, y: 166, diameter: 26 },
  { x: 168, y: 156, diameter: 32 },
  { x: 196, y: 170, diameter: 24 },
  { x: 216, y: 164, diameter: 28 },
  { x: 234, y: 174, diameter: 20 },
];

// ============================================================================
// Derived values
// ============================================================================

/** The face shows no seconds, so it reads the time cut to the minute and wakes once a minute. */
const minute_now = computed(() => start_of_minute(now.value));

const time_text = computed(
  () => `${format_number(display_hour(minute_now.value), 2)}:${format_number(minute_of(minute_now.value), 2)}`,
);

const date_text = computed(() => to_upper(format_time(minute_now.value, "ddd D MMM YYYY")));

// ============================================================================
// Screen
// ============================================================================

/** A sky that pales towards the horizon, then the black park in front of it. */
function Skyline() {
  return (
    <>
      <Column x={0} y={0} width={240} height={50} background={HIGH_SKY} />
      <Column x={0} y={50} width={240} height={60} background={MIDDLE_SKY} />
      <Column x={0} y={110} width={240} height={45} background={LOW_SKY} />
      <Column x={0} y={155} width={240} height={35} background={HAZE} />
      {TREE_CROWNS.map((crown) => (
        <Column x={crown.x} y={crown.y} width={crown.diameter} height={crown.diameter} radius={crown.diameter} background={BLACK} />
      ))}
      <Column x={0} y={184} width={240} height={56} background={BLACK} />
    </>
  );
}

function ParkSkylineFace() {
  return (
    <Screen>
      <Skyline />

      {unread_notifications.value > 0 && <Icon x={4} y={2} icon={icons.notification} size={20} color={BLACK} />}
      <Row x={150} y={2} width={86} height={22} justify="end" gap={6}>
        {is_phone_connected.value && <Icon icon={icons.bluetooth} size={18} color={WHITE} />}
        {is_charging.value ? <Icon icon={icons.charging} size={18} color={WHITE} /> : <BatteryIcon single_color color={WHITE} />}
      </Row>

      <Label x={0} y={52} width={240} text_align="center" color={BLACK}>
        {date_text.value}
      </Label>
      <Label x={0} y={88} width={240} text_align="center" font="huge" color={BLACK}>
        {time_text.value}
      </Label>

      <Badge x={4} y={214} height={24} icon={icons.heart} icon_size={22} gap={5} color={WHITE}>
        {`${heart_rate.value} BPM`}
      </Badge>
      <StepsBadge x={140} y={214} width={96} height={24} icon_size={22} align="end" color={WHITE} />
    </Screen>
  );
}

export default define_app({
  id: "park-skyline",
  name: "Park Skyline",
  icon: icons.clock,
  kind: "face",
  screens: { main: ParkSkylineFace },
});
