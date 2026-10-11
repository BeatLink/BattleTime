// Infineat watch face, ported from InfiniTime's WatchFaceInfineat.
// Coloured diagonal stripes cover the left side, with a pine logo whose fill shows the battery charge.
// InfiniTime's pop-up colour and cover buttons are a settings screen here, opened by a long press.

import { computed } from "@preact/signals";
import {
  back,
  battery_percent,
  day_of,
  define_app,
  format_number,
  go,
  hour_of,
  is_charging,
  is_phone_connected,
  minute_of,
  now,
  persisted,
  second_of,
  settings,
  steps_today,
  unread_notifications,
  weekday_of,
  type Color,
} from "neotime";
import { icons } from "neotime/icons";
import { Button, Column, Icon, Label, Line, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const GRAY: Color = "#999999";

const WEEKDAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

const PALETTE_NAMES = ["Orange", "Blue", "Green", "Rainbow", "Gray", "Nord blue", "Nord green"];
const PALETTE_COUNT = 7;
const STRIPE_COUNT = 9;

/** The stripes of the side cover, each from one point to another with its own width. */
const STRIPE_FROM_X = [30, 26, 27, 12, 17, 14, 14, -20, -29];
const STRIPE_FROM_Y = [25, 167, 40, 182, 99, 81, 163, 124, 89];
const STRIPE_TO_X = [68, 43, 27, 65, 17, 40, 40, 25, 27];
const STRIPE_TO_Y = [-8, 216, 196, 249, 144, 127, 118, -11, 254];
const STRIPE_WIDTHS = [18, 15, 14, 22, 20, 18, 18, 52, 48];
/** Nine stripe colours for each palette, one palette after another. */
const PALETTE_COLORS: Color[] = [
  "#fd872b", "#db3316", "#6f1000", "#fd7a0a", "#ffffff", "#ffffff", "#ffffff", "#e85102", "#ea1c00",
  "#e7f8ff", "#2232d0", "#182a8b", "#e7f8ff", "#ffffff", "#ffffff", "#ffffff", "#5991ff", "#1636ff",
  "#b8ff9b", "#088608", "#004a00", "#b8ff9b", "#ffffff", "#ffffff", "#ffffff", "#62d515", "#007400",
  "#2da400", "#ac09c4", "#fe0303", "#0d57ff", "#ffffff", "#ffffff", "#ffffff", "#e0b900", "#e85102",
  "#eeeeee", "#98959b", "#191919", "#eeeeee", "#ffffff", "#ffffff", "#ffffff", "#919191", "#3a3a3a",
  "#c3daf2", "#4d78ce", "#153451", "#c3daf2", "#ffffff", "#ffffff", "#ffffff", "#5d8ad2", "#21518a",
  "#d5f0e9", "#238373", "#1d413f", "#d5f0e9", "#ffffff", "#ffffff", "#ffffff", "#2fb8a2", "#11705a",
];

const LOGO_TOP = 105;
const LOGO_HEIGHT = 32;

const palette = persisted("palette", 0);

const shows_side_cover = persisted("shows_side_cover", true);

// ============================================================================
// Derived values
// ============================================================================

const uses_24_hour_clock = computed(() => settings.uses_24_hour_clock.value);

const display_hour = computed(() => {
  const hour = hour_of(now.value);
  return uses_24_hour_clock.value ? hour : hour % 12 === 0 ? 12 : hour % 12;
});

/** The bottom of the dark bar that hides the drained part of the logo for a given charge. */
function cover_bottom_for(percent: number): number {
  return LOGO_TOP + 1 + Math.floor((LOGO_HEIGHT * (100 - percent)) / 100);
}

/** The cover off the charger, which reads no clock, so it redraws only when the charge changes. */
const resting_cover_bottom = computed(() => cover_bottom_for(battery_percent.value));

/** While charging the logo fills up again and again, a quarter of the way each second. */
const charging_cover_bottom = computed(() =>
  cover_bottom_for(
    Math.min(100, battery_percent.value + Math.floor(((100 - battery_percent.value) * (second_of(now.value) % 5)) / 4)),
  ),
);

/** The notification dot takes the palette's accent, or white without the side cover. */
const notification_color = computed<Color>(() =>
  shows_side_cover.value ? PALETTE_COLORS[palette.value * STRIPE_COUNT + 7] : "#ffffff",
);

// ============================================================================
// Handlers
// ============================================================================

function next_palette() {
  palette.value = (palette.value + 1) % PALETTE_COUNT;
}

function previous_palette() {
  palette.value = palette.value > 0 ? palette.value - 1 : PALETTE_COUNT - 1;
}

function toggle_side_cover() {
  shows_side_cover.value = !shows_side_cover.value;
}

// ============================================================================
// Screens
// ============================================================================

function PineLogo() {
  return (
    <>
      <Line
        points={[
          { x: 27, y: 106 },
          { x: 18, y: 120 },
          { x: 23, y: 120 },
          { x: 15, y: 133 },
          { x: 39, y: 133 },
          { x: 31, y: 120 },
          { x: 36, y: 120 },
          { x: 27, y: 106 },
        ]}
        thickness={3}
        color="#ffffff"
      />
      <Line points={[{ x: 27, y: 133 }, { x: 27, y: 138 }]} thickness={4} color="#ffffff" />
      {is_charging.value ? (
        <Line
          points={[{ x: 27, y: LOGO_TOP }, { x: 27, y: charging_cover_bottom.value }]}
          thickness={26}
          rounded={false}
          color="#000000"
        />
      ) : (
        <Line
          points={[{ x: 27, y: LOGO_TOP }, { x: 27, y: resting_cover_bottom.value }]}
          thickness={26}
          rounded={false}
          color="#000000"
        />
      )}
    </>
  );
}

function SideCover() {
  return (
    <>
      {STRIPE_WIDTHS.map((width, place) => (
        <Line
          points={[
            { x: STRIPE_FROM_X[place], y: STRIPE_FROM_Y[place] },
            { x: STRIPE_TO_X[place], y: STRIPE_TO_Y[place] },
          ]}
          thickness={width}
          rounded={false}
          color={PALETTE_COLORS[palette.value * STRIPE_COUNT + place]}
        />
      ))}
    </>
  );
}

function InfineatFace() {
  return (
    <Screen onLongPress={() => go("settings")}>
      {shows_side_cover.value && <SideCover />}
      <PineLogo />

      {unread_notifications.value > 0 && (
        <Column x={227} y={0} width={13} height={13} radius={7} background={notification_color.value} />
      )}

      <Label x={27} y={14} width={185} text_align="center" font="huge">
        {format_number(display_hour.value, 2)}
      </Label>
      <Label x={27} y={104} width={185} text_align="center" font="huge">
        {format_number(minute_of(now.value), 2)}
      </Label>
      {!uses_24_hour_clock.value && (
        <Label x={200} y={30}>
          {hour_of(now.value) >= 12 ? "PM" : "AM"}
        </Label>
      )}

      <Label x={176} y={108} width={64} text_align="center" font="small" color={GRAY}>
        {`${WEEKDAY_NAMES[weekday_of(now.value)]} ${format_number(day_of(now.value), 2)}`}
      </Label>
      {is_phone_connected.value && <Icon x={198} y={130} icon={icons.bluetooth} size={20} color={GRAY} />}

      <Row x={0} y={212} width={240} height={28} justify="center" gap={5}>
        <Icon icon={icons.steps} size={22} color={GRAY} />
        <Label color={GRAY}>{`${steps_today.value}`}</Label>
      </Row>
    </Screen>
  );
}

function Settings() {
  return (
    <Screen padding={8} gap={8} justify="center">
      <Row gap={6} height={60}>
        <Button width={60} height="100%" disabled={!shows_side_cover.value} onClick={previous_palette}>
          {"<"}
        </Button>
        <Column grow={1} align_items="center" gap={4}>
          <Label font="small" color="muted">Colours</Label>
          <Row gap={2}>
            <Column width={16} height={16} background={PALETTE_COLORS[palette.value * STRIPE_COUNT]} />
            <Column width={16} height={16} background={PALETTE_COLORS[palette.value * STRIPE_COUNT + 1]} />
            <Column width={16} height={16} background={PALETTE_COLORS[palette.value * STRIPE_COUNT + 2]} />
            <Column width={16} height={16} background={PALETTE_COLORS[palette.value * STRIPE_COUNT + 7]} />
          </Row>
          <Label font="small">{PALETTE_NAMES[palette.value]}</Label>
        </Column>
        <Button width={60} height="100%" disabled={!shows_side_cover.value} onClick={next_palette}>
          {">"}
        </Button>
      </Row>
      <Button height={50} onClick={toggle_side_cover}>
        {shows_side_cover.value ? "Side cover: On" : "Side cover: Off"}
      </Button>
      <Button height={50} background="accent" onClick={back}>
        <Icon icon={icons.check} size={28} />
      </Button>
    </Screen>
  );
}

export default define_app({
  id: "infineat",
  name: "Infineat",
  icon: icons.clock,
  kind: "face",
  screens: { main: InfineatFace, settings: Settings },
});
