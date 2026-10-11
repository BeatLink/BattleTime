// Four in a Row against the watch, from wasp-os's four_in_a_row.
// The menu picks a level and shows the share of games won at it; swipe down to play. Tap a column to
// aim your red counter and swipe down to drop it; the watch answers with yellow, looking ahead one
// move more for each level, taking any win and blocking any threat. Swipe down after the game ends.
// The watch's search runs a few steps per call, carried on by a fast tick, so no call runs long.

import { computed, signal } from "@preact/signals";
import {
  back,
  clamp,
  define_app,
  every,
  go,
  persisted,
  random_int,
  vibrate,
  type ClickEvent,
  type Color,
  type SwipeEvent,
} from "neotime";
import { Card } from "neotime/components";
import { icons } from "neotime/icons";
import { Button, Column, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// Rules
// ============================================================================

const COLUMN_COUNT = 7;
const ROW_COUNT = 6;
const CELL_COUNT = 42;

/** Each row holds the seven columns and an eighth that stays empty, so a line running off a side stops there. */
export const ROW_WIDTH = 8;
const BOARD_SIZE = 48;

const EMPTY = 0;
const PLAYER = 1;
const COMPUTER = 2;

/** Every place on the board, the empty eighth column included, numbered from the bottom row. */
const BOARD_PLACES = [
  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
  32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
];

/** The 42 places a counter can fill. */
const CELLS = [
  0, 1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 28, 29, 30, 32, 33, 34, 35,
  36, 37, 38, 40, 41, 42, 43, 44, 45, 46,
];

const COLUMNS = [0, 1, 2, 3, 4, 5, 6];

/** Columns from the middle outwards, so ties go to the stronger central moves. */
const SEARCH_ORDER = [3, 2, 4, 1, 5, 0, 6];
const REVERSED_SEARCH_ORDER = [6, 0, 5, 1, 4, 2, 3];

const STEPS_ALONG_LINE = [1, 2, 3];

/** How far one step along a line moves on the board, in places. */
const RIGHT = 1;
const LEFT = -1;
const DOWN = -ROW_WIDTH;
const UP_RIGHT = ROW_WIDTH + 1;
const DOWN_LEFT = -ROW_WIDTH - 1;
const UP_LEFT = ROW_WIDTH - 1;
const DOWN_RIGHT = -ROW_WIDTH + 1;

/** The board is drawn in cells of this size, the top row holding the counter being aimed. */
const CELL_SIZE = 34;
const BORDER = 1;

const WIN_SCORE = 100;
const WORST_SCORE = -1000;

export const EMPTY_BOARD = BOARD_PLACES.map(() => EMPTY);
export const EMPTY_HEIGHTS = [0, 0, 0, 0, 0, 0, 0];

/** How many of a piece lie in a row from a cell, not counting the cell itself, up to three. */
function run_length(board: number[], cell: number, step: number, piece: number): number {
  for (const distance of STEPS_ALONG_LINE) {
    const next = cell + step * distance;
    if (next < 0 || next >= BOARD_SIZE || board[next] !== piece) return distance - 1;
  }
  return 3;
}

/** Whether putting a piece on a cell would make four in a row through it. */
export function wins_at(board: number[], cell: number, piece: number): boolean {
  return (
    run_length(board, cell, RIGHT, piece) + run_length(board, cell, LEFT, piece) >= 3 ||
    run_length(board, cell, DOWN, piece) >= 3 ||
    run_length(board, cell, UP_RIGHT, piece) + run_length(board, cell, DOWN_LEFT, piece) >= 3 ||
    run_length(board, cell, UP_LEFT, piece) + run_length(board, cell, DOWN_RIGHT, piece) >= 3
  );
}

/** The first column in the order given where a piece would win at once, or -1. */
function winning_column(board: number[], heights: number[], piece: number, order: number[]): number {
  for (const column of order) {
    if (heights[column] < ROW_COUNT && wins_at(board, heights[column] * ROW_WIDTH + column, piece)) return column;
  }
  return -1;
}

function with_cell(board: number[], target: number, piece: number): number[] {
  return BOARD_PLACES.map((place) => (place === target ? piece : board[place]));
}

function with_height(heights: number[], column: number, height: number): number[] {
  return COLUMNS.map((other) => (other === column ? height : heights[other]));
}

/** A sooner win is worth more. */
function win_score(filled: number): number {
  return WIN_SCORE + CELL_COUNT - filled;
}

/** The watch moves at even depths of the search and the player at odd ones. */
function piece_at_depth(depth: number): number {
  return depth % 2 === 0 ? COMPUTER : PLAYER;
}

// ============================================================================
// State
// ============================================================================

const LEVELS = [0, 1, 2, 3];

const level = persisted("level", 0);

/** Games won and games played at each level. */
const wins = persisted<number[]>("wins", [0, 0, 0, 0]);
const games = persisted<number[]>("games", [0, 0, 0, 0]);

export const board = signal<number[]>(EMPTY_BOARD);
export const heights = signal<number[]>(EMPTY_HEIGHTS);
export const filled = signal(0);

/** The column the player's counter hangs over. */
const aimed_column = signal(3);

const outcome = signal<"playing" | "won" | "lost" | "draw">("playing");

// ============================================================================
// Search state
// ============================================================================

/** Search steps one call may take; each costs at most about 7,000 interpreter steps. */
const STEPS_PER_CALL = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/** One place per depth of the search, which looks at most four moves ahead. */
const DEPTHS = [0, 1, 2, 3];

/** The board and column heights the search tries its moves on. */
const trial = signal<number[]>(EMPTY_BOARD);
const trial_heights = signal<number[]>(EMPTY_HEIGHTS);

/** For each depth: the next place in the search order to try, the best score so far and the column to block, or -1. */
const depth_next = signal<number[]>([0, 0, 0, 0]);
const depth_best = signal<number[]>([0, 0, 0, 0]);
const depth_threat = signal<number[]>([-1, -1, -1, -1]);

/** The depth the search is working at, or -1 while it is not searching. */
export const search_depth = signal(-1);

/** How many moves ahead the search looks in all. */
const search_reach = signal(1);

/** The best column found so far, or -1 while no move is waiting to be played. */
export const chosen_column = signal(-1);

// ============================================================================
// Derived values
// ============================================================================

const is_thinking = computed(() => search_depth.value >= 0);

const won_percent = computed(() => {
  const played = games.value[level.value];
  return played === 0 ? 0 : Math.floor((100 * wins.value[level.value] + Math.floor(played / 2)) / played);
});

const outcome_text = computed(() =>
  outcome.value === "won" ? "You win!" : outcome.value === "lost" ? "You lose!" : outcome.value === "draw" ? "Draw" : "",
);

// ============================================================================
// Search
// ============================================================================

function with_depth_value(values: number[], depth: number, value: number): number[] {
  return DEPTHS.map((other) => (other === depth ? value : values[other]));
}

function play_trial(column: number, piece: number) {
  const row = trial_heights.value[column];
  trial.value = with_cell(trial.value, row * ROW_WIDTH + column, piece);
  trial_heights.value = with_height(trial_heights.value, column, row + 1);
}

function take_back_trial(column: number) {
  const row = trial_heights.value[column] - 1;
  trial.value = with_cell(trial.value, row * ROW_WIDTH + column, EMPTY);
  trial_heights.value = with_height(trial_heights.value, column, row);
}

/** Keeps a column's score at a depth if it beats the best so far; at the top it becomes the choice. */
function record_score(depth: number, column: number, score: number) {
  if (score > depth_best.value[depth]) {
    depth_best.value = with_depth_value(depth_best.value, depth, score);
    if (depth === 0) chosen_column.value = column;
  }
}

/** Takes back the move into a depth and scores it for the side that made it. */
function leave_depth(depth: number, value: number) {
  const column = SEARCH_ORDER[depth_next.value[depth - 1] - 1];
  take_back_trial(column);
  search_depth.value = depth - 1;
  record_score(depth - 1, column, -value);
}

/** Starts work on the position after a move: scored at once if the side to move wins or the search ends here. */
function enter_depth(depth: number) {
  const win = winning_column(trial.value, trial_heights.value, piece_at_depth(depth), SEARCH_ORDER);
  if (win >= 0) {
    leave_depth(depth, win_score(filled.value + depth));
  } else if (search_reach.value - depth === 1) {
    leave_depth(depth, 0);
  } else {
    depth_next.value = with_depth_value(depth_next.value, depth, 0);
    depth_best.value = with_depth_value(depth_best.value, depth, WORST_SCORE);
    depth_threat.value = with_depth_value(
      depth_threat.value,
      depth,
      winning_column(trial.value, trial_heights.value, piece_at_depth(depth + 1), REVERSED_SEARCH_ORDER),
    );
    search_depth.value = depth;
  }
}

/** Starts a search for the watch's move, looking `reach` moves ahead; a winning move is chosen at once. */
export function begin_search(reach: number) {
  trial.value = board.value;
  trial_heights.value = heights.value;
  search_reach.value = reach;
  chosen_column.value = winning_column(board.value, heights.value, COMPUTER, SEARCH_ORDER);
  if (chosen_column.value < 0) {
    depth_next.value = with_depth_value(depth_next.value, 0, 0);
    depth_best.value = with_depth_value(depth_best.value, 0, WORST_SCORE);
    depth_threat.value = with_depth_value(
      depth_threat.value,
      0,
      winning_column(board.value, heights.value, PLAYER, REVERSED_SEARCH_ORDER),
    );
    search_depth.value = 0;
  }
}

/** Tries the next column at the current depth, or finishes the depth once every column is tried. */
export function search_step() {
  const depth = search_depth.value;
  const place = depth_next.value[depth];
  if (place >= COLUMN_COUNT) {
    const value = depth_best.value[depth] === WORST_SCORE ? 0 : depth_best.value[depth];
    if (depth === 0) search_depth.value = -1;
    else leave_depth(depth, value);
    return;
  }
  depth_next.value = with_depth_value(depth_next.value, depth, place + 1);
  const column = SEARCH_ORDER[place];
  if (trial_heights.value[column] >= ROW_COUNT) return;
  if (depth_threat.value[depth] >= 0 && depth_threat.value[depth] !== column) return;
  if (search_reach.value - depth === 1) {
    record_score(depth, column, 0);
  } else {
    play_trial(column, piece_at_depth(depth));
    enter_depth(depth + 1);
  }
}

// ============================================================================
// Handlers
// ============================================================================

function count_game(won: boolean) {
  games.value = LEVELS.map((other) => (other === level.value ? games.value[other] + 1 : games.value[other]));
  if (won) wins.value = LEVELS.map((other) => (other === level.value ? wins.value[other] + 1 : wins.value[other]));
}

/** Drops a piece into a column with room, and ends the game if it wins or fills the board. */
function place(column: number, piece: number) {
  const cell = heights.value[column] * ROW_WIDTH + column;
  const wins_game = wins_at(board.value, cell, piece);
  board.value = with_cell(board.value, cell, piece);
  heights.value = with_height(heights.value, column, heights.value[column] + 1);
  filled.value = filled.value + 1;
  if (wins_game) {
    outcome.value = piece === PLAYER ? "won" : "lost";
    count_game(piece === PLAYER);
    vibrate(piece === PLAYER ? 100 : 300);
  } else if (filled.value === CELL_COUNT) {
    outcome.value = "draw";
    count_game(false);
  }
}

/** Carries the search on for a few steps, and plays the chosen column once it is done. */
function think() {
  if (search_depth.value < 0 && chosen_column.value < 0) return;
  for (const _step of STEPS_PER_CALL) {
    if (search_depth.value >= 0) search_step();
  }
  if (search_depth.value < 0 && chosen_column.value >= 0) {
    place(chosen_column.value, COMPUTER);
    chosen_column.value = -1;
  }
}

function computer_move() {
  if (filled.value < 3) {
    place(random_int(0, COLUMN_COUNT - 1), COMPUTER);
  } else {
    begin_search(level.value + 1);
    think();
  }
}

function start_game() {
  board.value = EMPTY_BOARD;
  heights.value = EMPTY_HEIGHTS;
  filled.value = 0;
  aimed_column.value = 3;
  outcome.value = "playing";
  search_depth.value = -1;
  chosen_column.value = -1;
  if (games.value[level.value] % 2 === 1) computer_move();
  go("game");
}

function menu_swipe(event: SwipeEvent) {
  if (event.direction === "down") start_game();
  else if (event.direction === "right") back();
}

function game_swipe(event: SwipeEvent) {
  if (event.direction !== "down") {
    back();
  } else if (outcome.value !== "playing") {
    back();
  } else if (is_thinking.value || heights.value[aimed_column.value] === ROW_COUNT) {
    vibrate(50);
  } else {
    place(aimed_column.value, PLAYER);
    if (outcome.value === "playing") computer_move();
  }
}

function aim(event: ClickEvent) {
  aimed_column.value = clamp(Math.floor((event.x - BORDER) / CELL_SIZE), 0, COLUMN_COUNT - 1);
}

// ============================================================================
// Screens
// ============================================================================

const PIECE_SIZE = 30;
const PIECE_COLORS: Color[] = ["#000000", "#f80000", "#f8f800"];

function Menu() {
  return (
    <Screen onSwipe={menu_swipe} padding={6} gap={6}>
      <Card onClick={start_game}>
        <Label>Four in a Row</Label>
        <Label font="small" color="muted">Swipe down to play</Label>
      </Card>
      <Card>
        <Label font="small" color="muted">Level</Label>
        <Row gap={6}>
          {LEVELS.map((choice) => (
            <Button
              grow={1}
              background={choice === level.value ? "accent" : "background"}
              onClick={() => (level.value = choice)}
            >
              {`${choice}`}
            </Button>
          ))}
        </Row>
      </Card>
      <Card grow={1}>
        <Label>{`Won ${won_percent.value}%`}</Label>
        <Label font="small" color="muted">{`at level ${level.value}`}</Label>
      </Card>
    </Screen>
  );
}

function Game() {
  return (
    <Screen onSwipe={game_swipe} onClick={aim}>
      <Column x={0} y={BORDER + CELL_SIZE} width={240} height={240 - BORDER - CELL_SIZE} background="#000080" />
      {outcome.value === "playing" && !is_thinking.value && (
        <Column
          x={BORDER + aimed_column.value * CELL_SIZE + 2}
          y={BORDER + 2}
          width={PIECE_SIZE}
          height={PIECE_SIZE}
          radius={15}
          background={PIECE_COLORS[PLAYER]}
        />
      )}
      {(outcome.value !== "playing" || is_thinking.value) && (
        <Label x={0} y={6} width={240} text_align="center">
          {is_thinking.value ? "Thinking..." : outcome_text.value}
        </Label>
      )}
      {CELLS.map((cell) => (
        <Column
          x={BORDER + (cell % ROW_WIDTH) * CELL_SIZE + 2}
          y={BORDER + (ROW_COUNT - Math.floor(cell / ROW_WIDTH)) * CELL_SIZE + 2}
          width={PIECE_SIZE}
          height={PIECE_SIZE}
          radius={15}
          background={PIECE_COLORS[board.value[cell]]}
        />
      ))}
    </Screen>
  );
}

export default define_app({
  id: "four-in-a-row",
  name: "4 in a row",
  icon: icons.game,
  screens: { main: Menu, game: Game },
  subscriptions: [every({ milliseconds: 100 }, think)],
});
