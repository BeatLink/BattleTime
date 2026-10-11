// Lite Pong watch face, ported from PinetimeLite's WatchFacePong.
// Two rackets play an endless rally under the time; each moves toward the ball only once it is coming its way.
// A missed ball waits a while, then the other side serves again from where its racket stands.

import { signal } from "@preact/signals";
import {
  battery_percent,
  define_app,
  every,
  format_number,
  format_time,
  hour_of,
  is_charging,
  minute_of,
  now,
  random_int,
  settings,
  type Color,
} from "neotime";
import { icons } from "neotime/icons";
import { Column, Icon, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const BACKGROUND: Color = "#001020";
const RACKET_COLOR: Color = "#ff6600";
const BALL_COLOR: Color = "#ffff00";

const BALL_STEP = 6;
const RACKET_REACH = 12;
const RACKET_WIDTH = 10;
const RACKET_HEIGHT = 2 * RACKET_REACH + 8;
const BALL_SIZE = 10;

/** The pause after a missed ball before the next serve, in game steps. */
const SERVE_DELAY_STEPS = 70;

/** One game step, as PinetimeLite runs it. */
const STEP_MILLISECONDS = 500;

/** Where the ball and rackets are; positions are their centres. */
const ball_x = signal(120);
const ball_y = signal(120);
const left_racket_y = signal(120);
const right_racket_y = signal(120);

/** Whether the ball moves right, and whether it moves down, up or straight across. */
let moves_right = true;
let vertical: "up" | "down" | "level" = "down";

/** How steeply the ball climbs or falls, from 1 (steep) to 4 (shallow). */
let shallowness = 1;

let serve_delay = 0;

// ============================================================================
// Game
// ============================================================================

/** Picks a new direction and slope for the ball after a hit or a serve. */
function pick_bounce() {
  const roll = random_int(0, 359);
  vertical = roll < 100 ? "up" : roll < 240 ? "down" : "level";
  shallowness = Math.max(1, random_int(0, 359) % 5);
}

/** Moves a racket one step toward a target height, keeping it on the screen. */
function chase(racket: typeof left_racket_y, target: number) {
  const lowest = RACKET_REACH + 3;
  const highest = 240 - lowest;
  if (racket.value > target) racket.value = Math.max(lowest, racket.value - BALL_STEP);
  if (racket.value < target) racket.value = Math.min(highest, racket.value + BALL_STEP);
}

function step() {
  if (serve_delay > 0) {
    serve_delay -= 1;
  } else {
    ball_x.value += moves_right ? BALL_STEP : -BALL_STEP;
    if (vertical === "down") ball_y.value += Math.floor(BALL_STEP / shallowness);
    if (vertical === "up") ball_y.value -= Math.floor(BALL_STEP / shallowness);
  }

  let missed_by: "left" | "right" | null = null;
  const near_left = Math.abs(ball_y.value - left_racket_y.value) <= RACKET_REACH;
  const near_right = Math.abs(ball_y.value - right_racket_y.value) <= RACKET_REACH;
  if (ball_x.value <= 2 * BALL_STEP && near_left) {
    moves_right = true;
    pick_bounce();
  } else if (ball_x.value <= 0) {
    missed_by = "left";
  }
  if (ball_x.value >= 240 - 2 * BALL_STEP && near_right) {
    moves_right = false;
    pick_bounce();
  } else if (ball_x.value >= 240 - BALL_STEP) {
    missed_by = "right";
  }

  if (serve_delay === 0) {
    if (!moves_right && ball_x.value < 160) chase(left_racket_y, ball_y.value);
    if (moves_right && ball_x.value > 80) chase(right_racket_y, ball_y.value);
  }

  if (ball_y.value <= BALL_STEP) vertical = "down";
  if (ball_y.value >= 240 - BALL_STEP) vertical = "up";

  if (missed_by) {
    pick_bounce();
    const right_serves = missed_by === "left";
    moves_right = !right_serves;
    vertical = right_serves ? "down" : "up";
    ball_x.value = right_serves ? 228 : 12;
    ball_y.value = right_serves ? right_racket_y.value : left_racket_y.value;
    serve_delay = SERVE_DELAY_STEPS;
  }
}

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

function Racket(props: { x: number; center_y: number }) {
  return (
    <Column
      x={props.x}
      y={props.center_y - RACKET_HEIGHT / 2}
      width={RACKET_WIDTH}
      height={RACKET_HEIGHT}
      radius={RACKET_WIDTH / 2}
      background={RACKET_COLOR}
    />
  );
}

/** The ball's shadow falls further away the nearer the ball is to the middle, as if it flew higher there. */
function shadow_offset() {
  return 15 - Math.floor(Math.abs(120 - ball_x.value) / 10);
}

function LitePongFace() {
  const hour = hour_of(now.value);
  const hour_text = settings.uses_24_hour_clock.value ? format_number(hour, 2) : `${hour % 12 === 0 ? 12 : hour % 12}`;
  const shadow = shadow_offset();
  return (
    <Screen background={BACKGROUND}>
      <Label x={8} y={8} width={104} text_align="center" font="huge">
        {hour_text}
      </Label>
      <Label x={128} y={8} width={104} text_align="center" font="huge">
        {format_number(minute_of(now.value), 2)}
      </Label>

      <Racket x={2} center_y={left_racket_y.value} />
      <Racket x={240 - 2 - RACKET_WIDTH} center_y={right_racket_y.value} />
      <Column
        x={ball_x.value - BALL_SIZE / 2 + shadow}
        y={ball_y.value - BALL_SIZE / 2 + shadow}
        width={BALL_SIZE}
        height={BALL_SIZE}
        radius={BALL_SIZE / 2}
        background="#000000"
      />
      <Column
        x={ball_x.value - BALL_SIZE / 2}
        y={ball_y.value - BALL_SIZE / 2}
        width={BALL_SIZE}
        height={BALL_SIZE}
        radius={BALL_SIZE / 2}
        background={BALL_COLOR}
      />

      <Label x={4} y={216}>{format_time(now.value, "ddd, DD MMM")}</Label>
      <Row x={200} y={218} width={36} justify="end">
        {is_charging.value ? <Icon icon={icons.charging} size={20} /> : <BatteryLevel />}
      </Row>
    </Screen>
  );
}

export default define_app({
  id: "lite-pong",
  name: "Lite Pong",
  icon: icons.game,
  kind: "face",
  screens: { main: LitePongFace },
  subscriptions: [every({ milliseconds: STEP_MILLISECONDS }, step)],
});
