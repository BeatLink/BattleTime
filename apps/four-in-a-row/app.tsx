// Four in a Row against the watch, from wasp-os's four_in_a_row.
// The menu picks a level and shows the share of games won at it; swipe down to play. Tap a column to
// aim your red counter and swipe down to drop it; the watch answers with yellow, looking ahead one
// move more for each level, taking any win and blocking any threat. Swipe down after the game ends.

import { computed, signal } from "@preact/signals";
import {
  back,
  clamp,
  define_app,
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

const EMPTY = 0;
const PLAYER = 1;
const COMPUTER = 2;

/** Cells are numbered across each row, starting from the bottom row. */
const CELLS = [
  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
  32, 33, 34, 35, 36, 37, 38, 39, 40, 41,
];

const COLUMNS = [0, 1, 2, 3, 4, 5, 6];

/** Columns from the middle outwards, so ties go to the stronger central moves. */
const SEARCH_ORDER = [3, 2, 4, 1, 5, 0, 6];

const STEPS_ALONG_LINE = [1, 2, 3];

/** The board is drawn in cells of this size, the top row holding the counter being aimed. */
const CELL_SIZE = 34;
const BORDER = 1;

const WIN_SCORE = 100;
const WORST_SCORE = -1000;

export const EMPTY_BOARD = CELLS.map(() => EMPTY);
export const EMPTY_HEIGHTS = [0, 0, 0, 0, 0, 0, 0];

/** How many of a piece lie in a row from a cell, not counting the cell itself, up to three. */
function count_along(
  board: number[],
  column: number,
  row: number,
  column_step: number,
  row_step: number,
  piece: number,
): number {
  let count = 0;
  let still_counting = true;
  for (const distance of STEPS_ALONG_LINE) {
    const next_column = column + column_step * distance;
    const next_row = row + row_step * distance;
    const is_on_board = next_column >= 0 && next_column < COLUMN_COUNT && next_row >= 0 && next_row < ROW_COUNT;
    if (still_counting && is_on_board && board[next_row * COLUMN_COUNT + next_column] === piece) {
      count = count + 1;
    } else {
      still_counting = false;
    }
  }
  return count;
}

/** Whether putting a piece on a cell would make four in a row through it. */
export function wins_at(board: number[], column: number, row: number, piece: number): boolean {
  return (
    count_along(board, column, row, 1, 0, piece) + count_along(board, column, row, -1, 0, piece) >= 3 ||
    count_along(board, column, row, 0, -1, piece) >= 3 ||
    count_along(board, column, row, 1, 1, piece) + count_along(board, column, row, -1, -1, piece) >= 3 ||
    count_along(board, column, row, 1, -1, piece) + count_along(board, column, row, -1, 1, piece) >= 3
  );
}

function dropped(board: number[], column: number, row: number, piece: number): number[] {
  const target = row * COLUMN_COUNT + column;
  return CELLS.map((cell) => (cell === target ? piece : board[cell]));
}

function raised(heights: number[], column: number): number[] {
  return COLUMNS.map((other) => (other === column ? heights[other] + 1 : heights[other]));
}

/** A sooner win is worth more. */
function win_score(filled: number): number {
  return WIN_SCORE + CELL_COUNT - filled;
}

/** The column the other side must block, or -1 if it has no immediate win. */
function threat_column(board: number[], heights: number[], piece: number): number {
  let threat = -1;
  for (const column of SEARCH_ORDER) {
    if (heights[column] < ROW_COUNT && wins_at(board, column, heights[column], 3 - piece)) threat = column;
  }
  return threat;
}

function can_win_now(board: number[], heights: number[], piece: number): boolean {
  let can_win = false;
  for (const column of SEARCH_ORDER) {
    if (heights[column] < ROW_COUNT && wins_at(board, column, heights[column], piece)) can_win = true;
  }
  return can_win;
}

/** How good the board is for the side to move, looking one move ahead. */
function search_one(board: number[], heights: number[], filled: number, piece: number): number {
  return can_win_now(board, heights, piece) ? win_score(filled) : 0;
}

/** How good the board is for the side to move, looking two moves ahead. */
function search_two(board: number[], heights: number[], filled: number, piece: number): number {
  const winning = can_win_now(board, heights, piece);
  const threat = threat_column(board, heights, piece);
  let best = WORST_SCORE;
  if (!winning && filled < CELL_COUNT) {
    for (const column of SEARCH_ORDER) {
      if (heights[column] < ROW_COUNT && (threat < 0 || column === threat)) {
        const value = -search_one(
          dropped(board, column, heights[column], piece),
          raised(heights, column),
          filled + 1,
          3 - piece,
        );
        best = Math.max(best, value);
      }
    }
  }
  return winning ? win_score(filled) : best === WORST_SCORE ? 0 : best;
}

/** How good the board is for the side to move, looking three moves ahead. */
function search_three(board: number[], heights: number[], filled: number, piece: number): number {
  const winning = can_win_now(board, heights, piece);
  const threat = threat_column(board, heights, piece);
  let best = WORST_SCORE;
  if (!winning && filled < CELL_COUNT) {
    for (const column of SEARCH_ORDER) {
      if (heights[column] < ROW_COUNT && (threat < 0 || column === threat)) {
        const value = -search_two(
          dropped(board, column, heights[column], piece),
          raised(heights, column),
          filled + 1,
          3 - piece,
        );
        best = Math.max(best, value);
      }
    }
  }
  return winning ? win_score(filled) : best === WORST_SCORE ? 0 : best;
}

/** How good a board is for the side to move after a move, searching `depth` moves further. */
function value_after(board: number[], heights: number[], filled: number, piece: number, depth: number): number {
  return depth === 3
    ? search_three(board, heights, filled, piece)
    : depth === 2
      ? search_two(board, heights, filled, piece)
      : depth === 1
        ? search_one(board, heights, filled, piece)
        : 0;
}

/** The best column for a piece, looking `depth` moves ahead in all. */
export function choose_column(board: number[], heights: number[], filled: number, piece: number, depth: number): number {
  const threat = threat_column(board, heights, piece);
  let best = WORST_SCORE;
  let best_column = -1;
  for (const column of SEARCH_ORDER) {
    if (heights[column] < ROW_COUNT && (threat < 0 || column === threat)) {
      const value = wins_at(board, column, heights[column], piece)
        ? win_score(filled)
        : -value_after(
            dropped(board, column, heights[column], piece),
            raised(heights, column),
            filled + 1,
            3 - piece,
            depth - 1,
          );
      if (value > best) {
        best = value;
        best_column = column;
      }
    }
  }
  return best_column;
}

// ============================================================================
// State
// ============================================================================

const LEVELS = [0, 1, 2, 3];

const level = persisted("level", 0);

/** Games won and games played at each level. */
const wins = persisted<number[]>("wins", [0, 0, 0, 0]);
const games = persisted<number[]>("games", [0, 0, 0, 0]);

const board = signal<number[]>(EMPTY_BOARD);
const heights = signal<number[]>(EMPTY_HEIGHTS);
const filled = signal(0);

/** The column the player's counter hangs over. */
const aimed_column = signal(3);

const outcome = signal<"playing" | "won" | "lost" | "draw">("playing");

// ============================================================================
// Derived values
// ============================================================================

const won_percent = computed(() => {
  const played = games.value[level.value];
  return played === 0 ? 0 : Math.floor((100 * wins.value[level.value] + Math.floor(played / 2)) / played);
});

const outcome_text = computed(() =>
  outcome.value === "won" ? "You win!" : outcome.value === "lost" ? "You lose!" : outcome.value === "draw" ? "Draw" : "",
);

// ============================================================================
// Handlers
// ============================================================================

function count_game(won: boolean) {
  games.value = LEVELS.map((other) => (other === level.value ? games.value[other] + 1 : games.value[other]));
  if (won) wins.value = LEVELS.map((other) => (other === level.value ? wins.value[other] + 1 : wins.value[other]));
}

/** Drops a piece into a column with room, and ends the game if it wins or fills the board. */
function place(column: number, piece: number) {
  const row = heights.value[column];
  const wins_game = wins_at(board.value, column, row, piece);
  board.value = dropped(board.value, column, row, piece);
  heights.value = raised(heights.value, column);
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

function computer_move() {
  const column =
    filled.value < 3
      ? random_int(0, COLUMN_COUNT - 1)
      : choose_column(board.value, heights.value, filled.value, COMPUTER, level.value + 1);
  if (column >= 0) place(column, COMPUTER);
}

function start_game() {
  board.value = EMPTY_BOARD;
  heights.value = EMPTY_HEIGHTS;
  filled.value = 0;
  aimed_column.value = 3;
  outcome.value = "playing";
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
  } else if (heights.value[aimed_column.value] === ROW_COUNT) {
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
      {outcome.value === "playing" && (
        <Column
          x={BORDER + aimed_column.value * CELL_SIZE + 2}
          y={BORDER + 2}
          width={PIECE_SIZE}
          height={PIECE_SIZE}
          radius={15}
          background={PIECE_COLORS[PLAYER]}
        />
      )}
      {outcome.value !== "playing" && (
        <Label x={0} y={6} width={240} text_align="center">
          {outcome_text.value}
        </Label>
      )}
      {CELLS.map((cell) => (
        <Column
          x={BORDER + (cell % COLUMN_COUNT) * CELL_SIZE + 2}
          y={BORDER + (ROW_COUNT - Math.floor(cell / COLUMN_COUNT)) * CELL_SIZE + 2}
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
});
