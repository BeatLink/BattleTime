// Snake, from wasp-os's snake.
// Swipes steer the snake to the food on a 16 by 16 board; hitting a wall or itself ends the game with a
// buzz and the high score, a tap starts again and a swipe leaves. The snake can no longer turn straight
// back into itself, and its first meal makes it grow like every other.

import { computed, signal } from "@preact/signals";
import { back, define_app, every, persisted, random_int, vibrate, type SwipeEvent } from "neotime";
import { icons } from "neotime/icons";
import { Column, Label, Screen } from "neotime/widgets";

// ============================================================================
// Rules
// ============================================================================

const BOARD_SIZE = 16;
const CELL_SIZE = 15;
const LAST_CELL = BOARD_SIZE * BOARD_SIZE - 1;
const START_CELL = 8 * BOARD_SIZE + 8;

/** The longest the snake can grow, one widget for each part of it. */
const LONGEST_SNAKE = 64;

/** Places along the snake, head first. */
const SEGMENTS = [
  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30,
  31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58,
  59, 60, 61, 62, 63,
];

/** Further places to try when the food would land on the snake. */
const FOOD_ATTEMPTS = [0, 1, 2, 3, 4, 5, 6, 7];

const STARTING_BODY = SEGMENTS.map(() => START_CELL);

/** How far one step moves the head, in cells numbered across each row. */
const STEP_FOR_DIRECTION = { up: -BOARD_SIZE, down: BOARD_SIZE, left: -1, right: 1 };

/** Whether a cell is one of the first `length` places of the snake. */
function is_on_snake(body: number[], length: number, cell: number): boolean {
  let found = false;
  for (const segment of SEGMENTS) {
    if (segment < length && body[segment] === cell) found = true;
  }
  return found;
}

/** Whether moving one step from a cell runs into a wall. */
export function hits_wall(cell: number, step: number): boolean {
  const column = cell % BOARD_SIZE;
  return (
    (step === -1 && column === 0) ||
    (step === 1 && column === BOARD_SIZE - 1) ||
    (step === -BOARD_SIZE && cell < BOARD_SIZE) ||
    (step === BOARD_SIZE && cell > LAST_CELL - BOARD_SIZE)
  );
}

/** A random cell off the snake, giving up after a few tries on a crowded board. */
function food_cell(body: number[], length: number): number {
  let cell = random_int(0, LAST_CELL);
  for (const attempt of FOOD_ATTEMPTS) {
    if (is_on_snake(body, length, cell)) cell = random_int(0, LAST_CELL);
  }
  return cell;
}

// ============================================================================
// State
// ============================================================================

/** Cells the snake covers, head first; only the first `length` count. */
const body = signal<number[]>(STARTING_BODY);

const length = signal(1);

/** The step the head takes each tick, 0 until the first swipe. */
const step_size = signal(0);

/** The step chosen since the last tick, so two quick swipes cannot reverse the snake. */
const next_step_size = signal(0);

const food = signal(START_CELL + 3 * BOARD_SIZE);

const is_running = signal(true);

/** The longest snake so far. */
const best_length = persisted("best_length", 1);

// ============================================================================
// Derived values
// ============================================================================

const score = computed(() => length.value - 1);

// ============================================================================
// Handlers
// ============================================================================

function restart() {
  body.value = STARTING_BODY;
  length.value = 1;
  step_size.value = 0;
  next_step_size.value = 0;
  food.value = food_cell(body.value, 1);
  is_running.value = true;
}

function end_game() {
  is_running.value = false;
  if (length.value > best_length.value) best_length.value = length.value;
  vibrate(200);
}

export function tick() {
  const step = next_step_size.value;
  const head = body.value[0];
  if (is_running.value && step !== 0) {
    step_size.value = step;
    const next_head = head + step;
    const eats = next_head === food.value;
    const next_length = eats ? Math.min(length.value + 1, LONGEST_SNAKE) : length.value;
    if (hits_wall(head, step) || is_on_snake(body.value, length.value - (eats ? 0 : 1), next_head)) {
      end_game();
    } else {
      body.value = SEGMENTS.map((segment) => (segment === 0 ? next_head : body.value[segment - 1]));
      length.value = next_length;
      if (eats) food.value = food_cell(body.value, next_length);
    }
  }
}

function steer(event: SwipeEvent) {
  const step = STEP_FOR_DIRECTION[event.direction];
  if (!is_running.value) back();
  else if (length.value === 1 || step !== -step_size.value) next_step_size.value = step;
}

function tap() {
  if (!is_running.value) restart();
}

// ============================================================================
// Screen
// ============================================================================

function Square(props: { cell: number; color: "text" | "accent" }) {
  return (
    <Column
      x={(props.cell % BOARD_SIZE) * CELL_SIZE + 1}
      y={Math.floor(props.cell / BOARD_SIZE) * CELL_SIZE + 1}
      width={CELL_SIZE - 2}
      height={CELL_SIZE - 2}
      background={props.color}
    />
  );
}

function Snake() {
  return (
    <Screen onSwipe={steer} onClick={tap}>
      {is_running.value && <Square cell={food.value} color="accent" />}
      {SEGMENTS.map(
        (segment) => is_running.value && segment < length.value && <Square cell={body.value[segment]} color="text" />,
      )}
      {!is_running.value && (
        <Column x={0} y={50} width={240} align_items="center" gap={4}>
          <Label font="large" color="danger">GAME</Label>
          <Label font="large" color="danger">OVER</Label>
          <Label color="danger">{`Score: ${score.value}`}</Label>
          <Label color="danger">{`Highscore: ${best_length.value - 1}`}</Label>
          <Label font="small" color="muted">Tap to play again</Label>
        </Column>
      )}
    </Screen>
  );
}

export default define_app({
  id: "snake",
  name: "Snake",
  icon: icons.game,
  screens: { main: Snake },
  subscriptions: [every({ milliseconds: 250 }, tick)],
});
