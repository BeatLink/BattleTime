// Four in a Row against the watch, from wasp-os's four_in_a_row.
// The menu picks a level and shows the share of games won at it; swipe down to play. Tap a column to
// aim your red counter and swipe down to drop it; the watch answers with yellow, looking ahead one
// move more for each level, taking any win and blocking any threat. Swipe down after the game ends.
// The watch's search runs a few steps per call, carried on by a fast tick, so no call runs long.

import {
  back,
  clamp,
  computed,
  define_app,
  go,
  line_through,
  on,
  persisted,
  random_int,
  range,
  signal,
  vibrate,
  type ClickEvent,
  type Color,
  type SwipeEvent,
} from "battletime";
import { Card } from "battletime/components";
import { icons } from "battletime/icons";
import { Button, CellGrid, Column, Label, Row, Screen } from "battletime/widgets";

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
const BOARD_PLACES = range(BOARD_SIZE);

const COLUMNS = range(COLUMN_COUNT);

/** The 42 places a counter can fill, in drawing order: across each row from the top left. */
const DRAWN_PLACES = range(CELL_COUNT).map(
  (cell) => (ROW_COUNT - 1 - Math.floor(cell / COLUMN_COUNT)) * ROW_WIDTH + (cell % COLUMN_COUNT),
);

/**
 * Columns from the middle outwards, so ties go to the stronger central moves. A win check takes the last
 * match, so it runs over the reversed order to find the first.
 */
const SEARCH_ORDER = [3, 2, 4, 1, 5, 0, 6];
const REVERSED_SEARCH_ORDER = [6, 0, 5, 1, 4, 2, 3];

/** Each column and row of the board is this many pixels apart, the top row holding the counter being aimed. */
const CELL_SIZE = 34;
const BORDER = 1;

const WIN_SCORE = 100;
const WORST_SCORE = -1000;

export const EMPTY_BOARD = BOARD_PLACES.map(() => EMPTY);
export const EMPTY_HEIGHTS = [0, 0, 0, 0, 0, 0, 0];

/** Whether putting a piece on a cell would make four in a row through it. */
export function wins_at(board: number[], cell: number, piece: number): boolean {
  return line_through(board, ROW_WIDTH, cell, piece) >= 4;
}

/** The last column in the order given where a piece would win at once, or -1. */
function winning_column(board: number[], heights: number[], piece: number, order: number[]): number {
  let found = -1;
  for (const column of order) {
    if (heights[column] < ROW_COUNT && wins_at(board, heights[column] * ROW_WIDTH + column, piece)) found = column;
  }
  return found;
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

const LEVELS = range(4);

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
const STEPS_PER_CALL = range(16);

/** The board and column heights the search tries its moves on. */
const trial = signal<number[]>(EMPTY_BOARD);
const trial_heights = signal<number[]>(EMPTY_HEIGHTS);

/** For each depth of the search, which looks at most four moves ahead: the next place in the search order to try, the best score so far and the column to block, or -1. */
const depth_next = signal<number[]>([0, 0, 0, 0]);
const depth_best = signal<number[]>([0, 0, 0, 0]);
const depth_threat = signal<number[]>([-1, -1, -1, -1]);

/** The depth the search is working at, or -1 while it is not searching. */
export const search_depth = signal(-1);

/** How many moves ahead the search looks in all. */
const search_reach = signal(1);

/** Whether a move has just been tried and the position after it waits to be looked at by the next step. */
const is_entering_depth = signal(false);

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

function play_trial(column: number, piece: number) {
  const row = trial_heights.value[column];
  trial.value[row * ROW_WIDTH + column] = piece;
  trial_heights.value[column] = row + 1;
}

function take_back_trial(column: number) {
  const row = trial_heights.value[column] - 1;
  trial.value[row * ROW_WIDTH + column] = EMPTY;
  trial_heights.value[column] = row;
}

/** Keeps a column's score at a depth if it beats the best so far; at the top it becomes the choice. */
function record_score(depth: number, column: number, score: number) {
  if (score > depth_best.value[depth]) {
    depth_best.value[depth] = score;
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
  // Both searches run outside any branch, which keeps the branches open inside them few.
  depth_next.value[depth] = 0;
  depth_best.value[depth] = WORST_SCORE;
  depth_threat.value[depth] = winning_column(trial.value, trial_heights.value, piece_at_depth(depth + 1), SEARCH_ORDER);
  search_depth.value = depth;
  const can_win = winning_column(trial.value, trial_heights.value, piece_at_depth(depth), REVERSED_SEARCH_ORDER) >= 0;
  if (can_win) leave_depth(depth, win_score(filled.value + depth));
  else if (search_reach.value - depth === 1) leave_depth(depth, 0);
}

/** Starts a search for the watch's move, looking `reach` moves ahead; a winning move is chosen at once. */
export function begin_search(reach: number) {
  trial.value = board.value;
  trial_heights.value = heights.value;
  search_reach.value = reach;
  is_entering_depth.value = false;
  chosen_column.value = winning_column(board.value, heights.value, COMPUTER, REVERSED_SEARCH_ORDER);
  if (chosen_column.value < 0) {
    depth_next.value[0] = 0;
    depth_best.value[0] = WORST_SCORE;
    depth_threat.value[0] = winning_column(board.value, heights.value, PLAYER, SEARCH_ORDER);
    search_depth.value = 0;
  }
}

/** Tries the next column at the current depth, or finishes the depth once every column is tried. */
export function search_step() {
  // Looking at a new position takes a step of its own, so its locals never add to the ones below.
  if (is_entering_depth.value) {
    is_entering_depth.value = false;
    enter_depth(search_depth.value + 1);
    return;
  }
  const depth = search_depth.value;
  const place = depth_next.value[depth];
  if (place >= COLUMN_COUNT) {
    const value = depth_best.value[depth] === WORST_SCORE ? 0 : depth_best.value[depth];
    if (depth === 0) search_depth.value = -1;
    else leave_depth(depth, value);
    return;
  }
  depth_next.value[depth] = place + 1;
  const column = SEARCH_ORDER[place];
  if (trial_heights.value[column] >= ROW_COUNT) return;
  if (depth_threat.value[depth] >= 0 && depth_threat.value[depth] !== column) return;
  if (search_reach.value - depth === 1) {
    record_score(depth, column, 0);
  } else {
    play_trial(column, piece_at_depth(depth));
    is_entering_depth.value = true;
  }
}

// ============================================================================
// Handlers
// ============================================================================

function count_game(won: boolean) {
  games.value[level.value] = games.value[level.value] + 1;
  if (won) wins.value[level.value] = wins.value[level.value] + 1;
}

/** Drops a piece into a column with room, and ends the game if it wins or fills the board. */
function place(column: number, piece: number) {
  const cell = heights.value[column] * ROW_WIDTH + column;
  const wins_game = wins_at(board.value, cell, piece);
  board.value[cell] = piece;
  heights.value[column] = heights.value[column] + 1;
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
  is_entering_depth.value = false;
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
/** The space around a counter inside its cell, so the drawn columns line up with the tapped ones. */
const PIECE_INSET = (CELL_SIZE - PIECE_SIZE) / 2;
const BOARD_COLOR: Color = "#000080";

/** Empty places show the board's blue, then the player's red and the watch's yellow. */
const PIECE_COLORS: Color[] = [BOARD_COLOR, "#f80000", "#f8f800"];

function Menu() {
  return (
    <Screen onSwipe={menu_swipe} padding={8} gap={6}>
      <Card padding={8} onClick={start_game}>
        <Label>Four in a Row</Label>
        <Label font="small" color="muted">Swipe down to play</Label>
      </Card>
      <Card padding={8}>
        <Label font="small" color="muted">Level</Label>
        <Row gap={6}>
          {LEVELS.map((choice) => (
            <Button
              grow={1}
              height={36}
              background={choice === level.value ? "accent" : "background"}
              onClick={() => (level.value = choice)}
            >
              {`${choice}`}
            </Button>
          ))}
        </Row>
      </Card>
      <Card grow={1} padding={8}>
        <Label>{`Won ${won_percent.value}%`}</Label>
        <Label font="small" color="muted">{`at level ${level.value}`}</Label>
      </Card>
    </Screen>
  );
}

function Game() {
  return (
    <Screen onSwipe={game_swipe} onClick={aim}>
      <Column x={0} y={BORDER + CELL_SIZE} width={240} height={240 - BORDER - CELL_SIZE} background={BOARD_COLOR} />
      {outcome.value === "playing" && !is_thinking.value && (
        <Column
          x={BORDER + aimed_column.value * CELL_SIZE + PIECE_INSET}
          y={BORDER + PIECE_INSET}
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
      <CellGrid
        x={BORDER + PIECE_INSET}
        y={BORDER + CELL_SIZE + PIECE_INSET}
        columns={COLUMN_COUNT}
        rows={ROW_COUNT}
        cell_size={PIECE_SIZE}
        cell_gap={CELL_SIZE - PIECE_SIZE}
        cell_shape="round"
        cells={DRAWN_PLACES.map((place) => board.value[place])}
        palette={PIECE_COLORS}
      />
    </Screen>
  );
}

on("tick", think, { every: { milliseconds: 100 }, while_open: true });

export default define_app({
  id: "four-in-a-row",
  name: "4 in a row",
  group: "games",
  icon: icons.game,
  screens: { main: Menu, game: Game },
});
