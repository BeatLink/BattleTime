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

function with_value(values: number[], place: number, value: number): number[] {
  return PLACES_IN_LINE.map((other) => (other === place ? value : values[other]));
}

/** Slides one line of four levels towards its start, merging each pair once; the fifth value is the points scored. */
export function slide_line(line: number[]): number[] {
  let packed = [0, 0, 0, 0];
  let packed_count = 0;
  for (const level of line) {
    if (level !== 0) {
      packed = with_value(packed, packed_count, level);
      packed_count = packed_count + 1;
    }
  }
  let result = [0, 0, 0, 0];
  let result_count = 0;
  let points = 0;
  let merged_with_previous = false;
  for (const place of PLACES_IN_LINE) {
    const level = packed[place];
    const next_level = place < 3 ? packed[place + 1] : 0;
    if (merged_with_previous) {
      merged_with_previous = false;
    } else if (level !== 0 && level === next_level) {
      result = with_value(result, result_count, level + 1);
      result_count = result_count + 1;
      points = points + TILE_VALUES[level + 1];
      merged_with_previous = true;
    } else if (level !== 0) {
      result = with_value(result, result_count, level);
      result_count = result_count + 1;
    }
  }
  return [result[0], result[1], result[2], result[3], points];
}

/** The board after a slide, followed by the points it scored as a seventeenth value. */
function slide_board(board: number[], lines: number[], place_of_cell: number[]): number[] {
  const first = slide_line([board[lines[0]], board[lines[1]], board[lines[2]], board[lines[3]]]);
  const second = slide_line([board[lines[4]], board[lines[5]], board[lines[6]], board[lines[7]]]);
  const third = slide_line([board[lines[8]], board[lines[9]], board[lines[10]], board[lines[11]]]);
  const fourth = slide_line([board[lines[12]], board[lines[13]], board[lines[14]], board[lines[15]]]);
  const in_line_order = [
    first[0], first[1], first[2], first[3],
    second[0], second[1], second[2], second[3],
    third[0], third[1], third[2], third[3],
    fourth[0], fourth[1], fourth[2], fourth[3],
  ];
  const points = first[4] + second[4] + third[4] + fourth[4];
  return CELLS_AND_POINTS.map((cell) => (cell < 16 ? in_line_order[place_of_cell[cell]] : points));
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

export function can_move(board: number[]): boolean {
  return (
    !boards_match(slide(board, "left"), board) ||
    !boards_match(slide(board, "right"), board) ||
    !boards_match(slide(board, "up"), board) ||
    !boards_match(slide(board, "down"), board)
  );
}

// ============================================================================
// State
// ============================================================================

const EMPTY_BOARD = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

const board = persisted<number[]>("board", EMPTY_BOARD);

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
  board.value = with_new_tile(with_new_tile(EMPTY_BOARD));
  score.value = 0;
}

function move(event: SwipeEvent) {
  const moved = slide(board.value, event.direction);
  const next_board = CELLS.map((cell) => moved[cell]);
  if (!is_asking_to_restart.value && !boards_match(next_board, board.value)) {
    board.value = with_new_tile(next_board);
    score.value = score.value + moved[16];
    if (score.value > best_score.value) best_score.value = score.value;
    if (!can_move(board.value)) vibrate(200);
  }
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
