// 2048, merged from InfiniTime's Twos and wasp-os's play2048.
// InfiniTime gives the score, the one-in-ten chance of a 4 and picking only empty cells; wasp-os gives
// the classic tile colours and tap to restart with a question. The board and best score persist.

import { computed, signal } from "@preact/signals";
import { define_app, persisted, random_int, vibrate, type Color, type SwipeEvent } from "neotime";
import { ConfirmDialog } from "neotime/components";
import { icons } from "neotime/icons";
import { Grid, Label, Row, Screen, Stack } from "neotime/widgets";

// ============================================================================
// Rules
// ============================================================================

/** The sixteen cells, numbered across each row from the top left. */
export const CELLS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/** The board's cells followed by one more place, which holds the points a slide scored. */
const CELLS_AND_POINTS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

const PLACES_IN_LINE = [0, 1, 2, 3];

/** Each direction's cells in four lines of four, every line listed from the edge the tiles move to. */
const LINES_LEFT = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
const LINES_RIGHT = [3, 2, 1, 0, 7, 6, 5, 4, 11, 10, 9, 8, 15, 14, 13, 12];
const LINES_UP = [0, 4, 8, 12, 1, 5, 9, 13, 2, 6, 10, 14, 3, 7, 11, 15];
const LINES_DOWN = [12, 8, 4, 0, 13, 9, 5, 1, 14, 10, 6, 2, 15, 11, 7, 3];

/** Where each cell sits in its direction's list above, to put the moved lines back on the board. */
const PLACE_LEFT = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
const PLACE_RIGHT = [3, 2, 1, 0, 7, 6, 5, 4, 11, 10, 9, 8, 15, 14, 13, 12];
const PLACE_UP = [0, 4, 8, 12, 1, 5, 9, 13, 2, 6, 10, 14, 3, 7, 11, 15];
const PLACE_DOWN = [3, 7, 11, 15, 2, 6, 10, 14, 1, 5, 9, 13, 0, 4, 8, 12];

/** Tiles are stored as levels, 1 for a 2, 2 for a 4 and so on, with 0 for an empty cell. */
const TILE_VALUES = [
  0, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072,
];

/** One of four values, picked by its place. */
function one_of_four(first: number, second: number, third: number, fourth: number, place: number): number {
  return place === 0 ? first : place === 1 ? second : place === 2 ? third : fourth;
}

/** The level at one place of a line of four after it slides towards its start, merging each pair once. */
function slid_level(first: number, second: number, third: number, fourth: number, wanted: number): number {
  let waiting = 0;
  let placed = 0;
  let found = 0;
  for (const place of PLACES_IN_LINE) {
    const level = one_of_four(first, second, third, fourth, place);
    if (level !== 0 && waiting === 0) {
      waiting = level;
    } else if (level !== 0) {
      if (placed === wanted) found = waiting === level ? level + 1 : waiting;
      placed = placed + 1;
      waiting = waiting === level ? 0 : level;
    }
  }
  return placed === wanted && waiting !== 0 ? waiting : found;
}

/** The points one line of four scores when it slides. */
function line_points(first: number, second: number, third: number, fourth: number): number {
  let waiting = 0;
  let points = 0;
  for (const place of PLACES_IN_LINE) {
    const level = one_of_four(first, second, third, fourth, place);
    if (level !== 0 && waiting === level) {
      points = points + TILE_VALUES[level + 1];
      waiting = 0;
    } else if (level !== 0) {
      waiting = level;
    }
  }
  return points;
}

/** Slides one line of four levels towards its start, merging each pair once; the fifth value is the points scored. */
export function slide_line(line: number[]): number[] {
  return [0, 1, 2, 3, 4].map((place) =>
    place < 4
      ? slid_level(line[0], line[1], line[2], line[3], place)
      : line_points(line[0], line[1], line[2], line[3]),
  );
}

/** A cell's level after a slide, from its own line in the direction's list. */
function slid_cell(board: number[], lines: number[], place_of_cell: number[], cell: number): number {
  const place = place_of_cell[cell];
  const start = place - (place % 4);
  return slid_level(
    board[lines[start]],
    board[lines[start + 1]],
    board[lines[start + 2]],
    board[lines[start + 3]],
    place - start,
  );
}

/** The points all four lines score in a slide. */
function board_points(board: number[], lines: number[]): number {
  let points = 0;
  for (const line of PLACES_IN_LINE) {
    const start = line * 4;
    points =
      points + line_points(board[lines[start]], board[lines[start + 1]], board[lines[start + 2]], board[lines[start + 3]]);
  }
  return points;
}

/** The board after a slide, followed by the points it scored as a seventeenth value. */
function slide_board(board: number[], lines: number[], place_of_cell: number[]): number[] {
  return CELLS_AND_POINTS.map((cell) =>
    cell < 16 ? slid_cell(board, lines, place_of_cell, cell) : board_points(board, lines),
  );
}

export function slide(board: number[], direction: SwipeEvent["direction"]): number[] {
  return direction === "left"
    ? slide_board(board, LINES_LEFT, PLACE_LEFT)
    : direction === "right"
      ? slide_board(board, LINES_RIGHT, PLACE_RIGHT)
      : direction === "up"
        ? slide_board(board, LINES_UP, PLACE_UP)
        : slide_board(board, LINES_DOWN, PLACE_DOWN);
}

function boards_match(first: number[], second: number[]): boolean {
  let match = true;
  for (const cell of CELLS) {
    if (first[cell] !== second[cell]) match = false;
  }
  return match;
}

/** Puts a 2, or one time in ten a 4, on a randomly chosen empty cell, if there is one. */
function with_new_tile(board: number[]): number[] {
  let empty_count = 0;
  for (const cell of CELLS) {
    if (board[cell] === 0) empty_count = empty_count + 1;
  }
  const chosen = random_int(0, Math.max(0, empty_count - 1));
  const level = random_int(1, 10) === 1 ? 2 : 1;
  let empty_seen = 0;
  let target = -1;
  for (const cell of CELLS) {
    if (board[cell] === 0) {
      if (empty_seen === chosen) target = cell;
      empty_seen = empty_seen + 1;
    }
  }
  return CELLS.map((cell) => (cell === target ? level : board[cell]));
}

/** Whether any slide would change the board: a tile with an empty cell somewhere, or two equal tiles side by side. */
export function can_move(board: number[]): boolean {
  let has_empty = false;
  let has_tile = false;
  let has_pair = false;
  for (const cell of CELLS) {
    const level = board[cell];
    if (level === 0) has_empty = true;
    else has_tile = true;
    if (level !== 0 && cell % 4 < 3 && board[cell + 1] === level) has_pair = true;
    if (level !== 0 && cell < 12 && board[cell + 4] === level) has_pair = true;
  }
  return (has_empty && has_tile) || has_pair;
}

// ============================================================================
// State
// ============================================================================

const EMPTY_BOARD = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

const board = persisted<number[]>("board", EMPTY_BOARD);

/** The board after the latest swipe's slide, followed by the points it scored. */
const slid = signal<number[]>([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

const score = persisted("score", 0);

const best_score = persisted("best_score", 0);

const is_asking_to_restart = signal(false);

// ============================================================================
// Derived values
// ============================================================================

const is_game_over = computed(() => !can_move(board.value));

// ============================================================================
// Handlers
// ============================================================================

export function start_game() {
  board.value = EMPTY_BOARD;
  board.value = with_new_tile(board.value);
  board.value = with_new_tile(board.value);
  score.value = 0;
}

function move(event: SwipeEvent) {
  if (is_asking_to_restart.value) return;
  slid.value = slide(board.value, event.direction);
  if (boards_match(slid.value, board.value)) return;
  board.value = CELLS.map((cell) => slid.value[cell]);
  board.value = with_new_tile(board.value);
  score.value = score.value + slid.value[16];
  if (score.value > best_score.value) best_score.value = score.value;
  if (!can_move(board.value)) vibrate(200);
}

function tap() {
  if (is_game_over.value) start_game();
  else is_asking_to_restart.value = true;
}

// ============================================================================
// Screen
// ============================================================================

/** Background and text colour for each level, from the original game's palette. */
const TILE_BACKGROUNDS: Color[] = [
  "surface", "#eee4da", "#ede0c8", "#f2b179", "#f59563", "#f67c5f", "#f65e3b", "#edcf72", "#edcc61",
  "#edc850", "#edc53f", "#edc22e", "#3c3a32", "#3c3a32", "#3c3a32", "#3c3a32", "#3c3a32", "#3c3a32",
];

const DARK_TILE_TEXT: Color = "#776e65";
const LIGHT_TILE_TEXT: Color = "#f9f6f2";

function Tile(props: { cell: number }) {
  const level = board.value[props.cell];
  return (
    <Stack background={TILE_BACKGROUNDS[level]} radius={6}>
      <Label font={level >= 14 ? "small" : "normal"} color={level <= 2 ? DARK_TILE_TEXT : LIGHT_TILE_TEXT}>
        {level === 0 ? "" : `${TILE_VALUES[level]}`}
      </Label>
    </Stack>
  );
}

function Twos() {
  return (
    <Screen onSwipe={move} onClick={tap}>
      <Row justify="between" height={28} padding={4}>
        <Row gap={6}>
          <Label font="small" color="muted">Score</Label>
          <Label font="small" color="warning">{`${score.value}`}</Label>
        </Row>
        <Label font="small" color={is_game_over.value ? "danger" : "muted"}>
          {is_game_over.value ? "Game over, tap" : `Best ${best_score.value}`}
        </Label>
      </Row>
      <Grid columns={4} gap={4} padding={4} width={240} height={212}>
        {CELLS.map((cell) => (
          <Tile cell={cell} />
        ))}
      </Grid>
      <ConfirmDialog open={is_asking_to_restart} question="Restart game?" onConfirm={start_game} />
    </Screen>
  );
}

export default define_app({
  id: "twos",
  name: "2048",
  icon: icons.game,
  screens: { main: Twos },
  onOpen: () => {
    if (boards_match(board.value, EMPTY_BOARD)) start_game();
  },
});
