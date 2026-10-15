// Snake, from wasp-os's snake.
// Swipes steer the snake to the food on a 16 by 16 board; hitting a wall or itself ends the game with a
// buzz and the high score, a tap starts again and a swipe leaves. The snake can no longer turn straight
// back into itself, and its first meal makes it grow like every other.

import {
  back,
  computed,
  define_app,
  on,
  persisted,
  random_int,
  range,
  signal,
  vibrate,
  type Color,
  type SwipeEvent,
} from "battletime";
import { icons } from "battletime/icons";
import { CellGrid, Column, Label, Screen } from "battletime/widgets";

// ============================================================================
// Rules
// ============================================================================

const BOARD_SIZE = 16;
const CELL_SIZE = 15;
const LAST_CELL = BOARD_SIZE * BOARD_SIZE - 1;
const START_CELL = 8 * BOARD_SIZE + 8;
const START_FOOD = START_CELL + 3 * BOARD_SIZE;

/** What a board cell holds: nothing, a part of the snake pointing the way to the next part towards the head, or food. */
const EMPTY = 0;
const POINTS_UP = 1;
const POINTS_DOWN = 2;
const POINTS_LEFT = 3;
const POINTS_RIGHT = 4;
const FOOD = 5;

/** How far one step moves the head, in cells numbered across each row. */
const STEP_FOR_DIRECTION = { up: -BOARD_SIZE, down: BOARD_SIZE, left: -1, right: 1 };

/** Further places to try when the food would land on the snake. */
const FOOD_ATTEMPTS = range(8);

/** Every cell of the board, numbered across each row from the top left. */
const CELLS = range(BOARD_SIZE * BOARD_SIZE);

export const STARTING_BOARD = range(BOARD_SIZE * BOARD_SIZE).map((cell) =>
  cell === START_CELL ? POINTS_UP : cell === START_FOOD ? FOOD : EMPTY,
);

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

/** The cell value for a part of the snake that points along a step. */
export function pointer_for_step(step: number): number {
  return step === -BOARD_SIZE ? POINTS_UP : step === BOARD_SIZE ? POINTS_DOWN : step === -1 ? POINTS_LEFT : POINTS_RIGHT;
}

/** The step a part of the snake points along. */
export function step_for_pointer(pointer: number): number {
  return pointer === POINTS_UP ? -BOARD_SIZE : pointer === POINTS_DOWN ? BOARD_SIZE : pointer === POINTS_LEFT ? -1 : 1;
}

function is_snake(value: number): boolean {
  return value >= POINTS_UP && value <= POINTS_RIGHT;
}

// ============================================================================
// State
// ============================================================================

/** The whole board; following the pointers from the tail leads along the snake to the head. */
export const board = signal<number[]>(STARTING_BOARD);
const head = signal(START_CELL);
const tail = signal(START_CELL);

export const length = signal(1);

/** The step the head takes each tick, 0 until the first swipe. */
const step_size = signal(0);

/** The step chosen since the last tick, so two quick swipes cannot reverse the snake. */
const next_step_size = signal(0);

export const is_running = signal(true);

/** The longest snake so far. */
const best_length = persisted("best_length", 1);

// ============================================================================
// Derived values
// ============================================================================

const score = computed(() => length.value - 1);

// ============================================================================
// Handlers
// ============================================================================

/** The first empty cell on the board, or -1 when the snake fills it. */
function first_empty_cell(): number {
  return board.value.indexOf(EMPTY);
}

/** Puts food on a random empty cell, or on the first empty one when a few random tries all land on the snake. */
function place_food() {
  let cell = random_int(0, LAST_CELL);
  for (const _attempt of FOOD_ATTEMPTS) {
    if (board.value[cell] !== EMPTY) cell = random_int(0, LAST_CELL);
  }
  if (board.value[cell] !== EMPTY) cell = first_empty_cell();
  if (cell >= 0) board.value[cell] = FOOD;
}

export function restart() {
  board.value = STARTING_BOARD;
  board.value[START_FOOD] = EMPTY;
  place_food();
  head.value = START_CELL;
  tail.value = START_CELL;
  length.value = 1;
  step_size.value = 0;
  next_step_size.value = 0;
  is_running.value = true;
}

function end_game() {
  is_running.value = false;
  if (length.value > best_length.value) best_length.value = length.value;
  vibrate(200);
}

/** Moves the head on a step; the tail follows unless the snake eats, and moving into the tail's old cell is safe. */
export function tick() {
  const step = next_step_size.value;
  if (!is_running.value || step === 0) return;
  step_size.value = step;
  const next_head = head.value + step;
  const ahead = hits_wall(head.value, step) ? EMPTY : board.value[next_head];
  const eats = ahead === FOOD;
  if (hits_wall(head.value, step) || (is_snake(ahead) && (eats || next_head !== tail.value))) {
    end_game();
    return;
  }
  board.value[head.value] = pointer_for_step(step);
  if (!eats) {
    const old_tail = tail.value;
    tail.value = old_tail + step_for_pointer(board.value[old_tail]);
    board.value[old_tail] = EMPTY;
  }
  board.value[next_head] = pointer_for_step(step);
  head.value = next_head;
  if (eats) {
    length.value = length.value + 1;
    place_food();
  }
}

export function steer(event: SwipeEvent) {
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

/** Empty cells, the four pointing parts of the snake, then food. */
const PALETTE: Color[] = ["background", "text", "text", "text", "text", "accent"];

function Snake() {
  return (
    <Screen onSwipe={steer} onClick={tap}>
      {is_running.value && (
        <CellGrid
          x={0}
          y={0}
          columns={BOARD_SIZE}
          rows={BOARD_SIZE}
          cell_size={CELL_SIZE}
          cells={board.value}
          palette={PALETTE}
        />
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

on("tick", tick, { every: { milliseconds: 250 }, while_open: true });

export default define_app({
  id: "snake",
  name: "Snake",
  group: "games",
  icon: icons.game,
  screens: { main: Snake },
});
