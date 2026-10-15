// 2048, merged from InfiniTime's Twos and wasp-os's play2048.
// InfiniTime gives the score, the one-in-ten chance of a 4 and picking only empty cells; wasp-os gives
// the classic tile colours and tap to restart with a question. The board and best score persist.

import {
  computed,
  count_of,
  count_where,
  define_app,
  nth_index_of,
  on,
  persisted,
  random_int,
  range,
  signal,
  vibrate,
  type Color,
  type SwipeDirection,
  type SwipeEvent,
} from "battletime";
import { ConfirmDialog } from "battletime/components";
import { icons } from "battletime/icons";
import { CellGrid, Label, Row, Screen } from "battletime/widgets";

// ============================================================================
// Rules
// ============================================================================

/** The sixteen cells, numbered across each row from the top left. */
export const CELLS = range(16);

/** The board is four cells wide. */
const WIDTH = 4;

/** Each tile's side and the space between tiles, in pixels, and the whole board's side. */
const CELL_SIZE = 45;
const CELL_GAP = 6;
const BOARD_SIDE = 4 * CELL_SIZE + 3 * CELL_GAP;

/** Tiles are stored as levels, 1 for a 2, 2 for a 4 and so on, with 0 for an empty cell. */
const TILE_VALUES = [
  0, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072,
];

/** The four places along a line of the board. */
const LINE_PLACES = range(WIDTH);

// ============================================================================
// Slides
// ============================================================================

/** Whether a slide moves tiles along rows, rather than columns. */
function is_across(direction: SwipeDirection): boolean {
  return direction === "left" || direction === "right";
}

/** The cell at a place along a line, counting places from the edge the tiles slide towards. */
function cell_along(direction: SwipeDirection, line: number, place: number): number {
  if (direction === "up") return place * WIDTH + line;
  if (direction === "down") return (WIDTH - 1 - place) * WIDTH + line;
  if (direction === "left") return line * WIDTH + place;
  return line * WIDTH + WIDTH - 1 - place;
}

/** The place of a cell along its line, counting from the edge the tiles slide towards. */
function place_along(direction: SwipeDirection, cell: number): number {
  if (direction === "up") return Math.floor(cell / WIDTH);
  if (direction === "down") return WIDTH - 1 - Math.floor(cell / WIDTH);
  if (direction === "left") return cell % WIDTH;
  return WIDTH - 1 - (cell % WIDTH);
}

/**
 * The level a cell holds after a slide. Its line's tiles pack towards the edge in order, and each pair of
 * equal tiles that meet joins once into one a level higher.
 */
export function slid_level(board: number[], direction: SwipeDirection, cell: number): number {
  const line = is_across(direction) ? Math.floor(cell / WIDTH) : cell % WIDTH;
  const wanted = place_along(direction, cell);
  let placed = 0;
  let waiting = 0;
  let found = 0;
  for (const place of LINE_PLACES) {
    const level = board[cell_along(direction, line, place)];
    if (level !== 0) {
      if (waiting === level) {
        if (placed === wanted) found = level + 1;
        placed += 1;
        waiting = 0;
      } else {
        if (waiting !== 0) {
          if (placed === wanted) found = waiting;
          placed += 1;
        }
        waiting = level;
      }
    }
  }
  if (waiting !== 0 && placed === wanted) found = waiting;
  return found;
}

/** The board after a slide. */
export function slid_board(board: number[], direction: SwipeDirection): number[] {
  return CELLS.map((cell) => slid_level(board, direction, cell));
}

/** Whether a slide would move or join any tile. */
export function slide_changes(board: number[], direction: SwipeDirection): boolean {
  return count_where(CELLS, (cell) => slid_level(board, direction, cell) !== board[cell]) > 0;
}

/** The points a slide scores: the value of each new tile its joins make. */
export function slide_points(board: number[], direction: SwipeDirection): number {
  let points = 0;
  for (const line of LINE_PLACES) {
    let waiting = 0;
    for (const place of LINE_PLACES) {
      const level = board[cell_along(direction, line, place)];
      if (level !== 0) {
        if (waiting === level) {
          points += 1 << (level + 1);
          waiting = 0;
        } else {
          waiting = level;
        }
      }
    }
  }
  return points;
}

/** Whether any slide would change the board: some tiles with an empty cell, or two equal tiles side by side. */
export function can_slide(board: number[]): boolean {
  const empty_count = count_of(board, 0);
  if (empty_count > 0) return empty_count < CELLS.length;
  return (
    count_where(
      CELLS,
      (cell) =>
        (cell % WIDTH < WIDTH - 1 && board[cell + 1] === board[cell]) ||
        (cell < CELLS.length - WIDTH && board[cell + WIDTH] === board[cell]),
    ) > 0
  );
}

// ============================================================================
// State
// ============================================================================

const EMPTY_BOARD = CELLS.map(() => 0);

export const board = persisted<number[]>("board", EMPTY_BOARD);

const score = persisted("score", 0);

const best_score = persisted("best_score", 0);

const is_asking_to_restart = signal(false);

// ============================================================================
// Derived values
// ============================================================================

const is_game_over = computed(() => !can_slide(board.value));

// ============================================================================
// Handlers
// ============================================================================

/** Puts a 2, or one time in ten a 4, on a randomly chosen empty cell, if there is one. */
function add_new_tile() {
  const empty_count = count_of(board.value, 0);
  if (empty_count === 0) return;
  const level = random_int(1, 10) === 1 ? 2 : 1;
  board.value[nth_index_of(board.value, 0, random_int(0, empty_count - 1))] = level;
}

export function start_game() {
  board.value = EMPTY_BOARD;
  add_new_tile();
  add_new_tile();
  score.value = 0;
}

/** A slide that changes the board scores its merges and brings a new tile; the slide reads the board as it was. */
export function move(event: SwipeEvent) {
  if (is_asking_to_restart.value) return;
  if (!slide_changes(board.value, event.direction)) return;
  const new_score = score.value + slide_points(board.value, event.direction);
  board.value = slid_board(board.value, event.direction);
  add_new_tile();
  score.value = new_score;
  if (new_score > best_score.value) best_score.value = new_score;
  if (!can_slide(board.value)) vibrate(200);
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

/** The number's colour on each level's tile: dark on the two lightest, light above them. */
const TILE_TEXT_COLORS: Color[] = TILE_BACKGROUNDS.map((_background, level) => (level <= 2 ? DARK_TILE_TEXT : LIGHT_TILE_TEXT));

function Twos() {
  return (
    <Screen padding={8} gap={6} onSwipe={move} onClick={tap}>
      <Row justify="space_between" height={20}>
        <Row gap={6}>
          <Label font="small" color="muted">Score</Label>
          <Label font="small" color="warning">{`${score.value}`}</Label>
        </Row>
        <Label font="small" color={is_game_over.value ? "danger" : "muted"}>
          {is_game_over.value ? "Game over" : `Best ${best_score.value}`}
        </Label>
      </Row>
      <Row grow={1} justify="center">
        <CellGrid
          width={BOARD_SIDE}
          height={BOARD_SIDE}
          columns={4}
          rows={4}
          cell_size={CELL_SIZE}
          cell_gap={CELL_GAP}
          cell_shape="rounded"
          cells={board.value}
          palette={TILE_BACKGROUNDS}
          show_numbers
          number_map={TILE_VALUES}
          number_colors={TILE_TEXT_COLORS}
        />
      </Row>
      <ConfirmDialog open={is_asking_to_restart} question="Restart game?" onConfirm={start_game} />
    </Screen>
  );
}

on("open", () => {
  if (count_of(board.value, 0) === CELLS.length) start_game();
});

export default define_app({
  id: "twos",
  name: "2048",
  group: "games",
  icon: icons.game,
  screens: { main: Twos },
});
