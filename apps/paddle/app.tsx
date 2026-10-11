// Paddle, from InfiniTime's Paddle.
// A ball bounces off the walls and the paddle on the left, which follows the finger up and down.
// Each return scores a point and a miss starts the count again; the best score persists.

import { signal } from "@preact/signals";
import { clamp, define_app, every, format_number, persisted, random_int, vibrate, type ClickEvent } from "neotime";
import { icons } from "neotime/icons";
import { Column, Label, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const SCREEN_SIZE = 240;
const BALL_SIZE = 16;
const PADDLE_HEIGHT = 60;
const PADDLE_WIDTH = 4;
const HALF_PADDLE = 30;
const FASTEST_DROP = 25;

/** How often the ball moves; the runtime ticks at most every 100 milliseconds, five times InfiniTime's period. */
const STEP_PERIOD = 100;

/** How far the ball moves across each step, five times InfiniTime's 2 pixels to keep its speed. */
const STEP_ACROSS = 10;

const BALL_START = Math.floor((SCREEN_SIZE - BALL_SIZE) / 2);

const ball_x = signal(BALL_START);
const ball_y = signal(BALL_START);

/** How far the ball moves each step, across and down. */
const step_across = signal(STEP_ACROSS);
const step_down = signal(15);

/** The paddle's centre, where the finger last touched. */
const paddle_center = signal(30);

const score = signal(0);

const best_score = persisted("best_score", 0);

// ============================================================================
// Handlers
// ============================================================================

export function step() {
  const x = ball_x.value + step_across.value;
  const y = ball_y.value + step_down.value;
  ball_x.value = x;
  ball_y.value = y;
  if (y <= 1 || y >= SCREEN_SIZE - BALL_SIZE - 2) step_down.value = -step_down.value;
  if (x >= SCREEN_SIZE - BALL_SIZE - 1) {
    step_across.value = -step_across.value;
    step_down.value = clamp(step_down.value + random_int(-5, 5), -FASTEST_DROP, FASTEST_DROP);
  }
  if (step_across.value < 0 && x <= PADDLE_WIDTH) {
    const quarter_ball = Math.floor(BALL_SIZE / 4);
    const is_level_with_paddle =
      y <= paddle_center.value + HALF_PADDLE - quarter_ball &&
      y >= paddle_center.value - HALF_PADDLE - BALL_SIZE + quarter_ball;
    if (x >= -quarter_ball - STEP_ACROSS && is_level_with_paddle) {
      step_across.value = -step_across.value;
      score.value = score.value + 1;
      if (score.value > best_score.value) best_score.value = score.value;
    } else if (x <= -BALL_SIZE * 2) {
      ball_x.value = BALL_START;
      ball_y.value = BALL_START;
      score.value = 0;
      vibrate(80);
    }
  }
}

function move_paddle(event: ClickEvent) {
  paddle_center.value = event.y;
}

/** Drags move the paddle, so a swipe must not leave the app; the button does that. */
function ignore_swipe() {}

// ============================================================================
// Screen
// ============================================================================

function Paddle() {
  const paddle_top = clamp(paddle_center.value - HALF_PADDLE, 1, SCREEN_SIZE - PADDLE_HEIGHT - 1);
  return (
    <Screen onPress={move_paddle} onDrag={move_paddle} onSwipe={ignore_swipe}>
      <Column x={0} y={0} width={SCREEN_SIZE} height={SCREEN_SIZE} border_width={1} border_color="text" />
      <Label x={0} y={10} width={SCREEN_SIZE} font="large" text_align="center">
        {format_number(score.value, 4)}
      </Label>
      <Label x={0} y={58} width={SCREEN_SIZE} font="small" color="muted" text_align="center">
        {`Best ${best_score.value}`}
      </Label>
      <Column x={0} y={paddle_top} width={PADDLE_WIDTH} height={PADDLE_HEIGHT} background="text" />
      <Column
        x={ball_x.value}
        y={ball_y.value}
        width={BALL_SIZE}
        height={BALL_SIZE}
        radius={Math.floor(BALL_SIZE / 2)}
        background="text"
      />
    </Screen>
  );
}

export default define_app({
  id: "paddle",
  name: "Paddle",
  icon: icons.game,
  screens: { main: Paddle },
  subscriptions: [every({ milliseconds: STEP_PERIOD }, step, { only_while_open: true })],
});
