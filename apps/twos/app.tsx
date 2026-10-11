// 2048, merged from InfiniTime's Twos and wasp-os's play2048.
// InfiniTime gives the score, the one-in-ten chance of a 4 and picking only empty cells; wasp-os gives
// the classic tile colours and tap to restart with a question. The board and best score persist.

import { computed, signal } from "@preact/signals";
import { define_app, persisted, random_int, range, vibrate, type Color, type SwipeEvent } from "neotime";
import { ConfirmDialog } from "neotime/components";
import { icons } from "neotime/icons";
import { Grid, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// Rules
// ============================================================================

/** The sixteen cells, numbered across each row from the top left. */
export const CELLS = range(16);

const PLACES_IN_LINE = range(4);

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

type Direction = SwipeEvent["direction"];

/** The cell at one place of a direction's list of lines. */
function line_cell(direction: Direction, place: number): number {
  return direction === "left"
    ? LINES_LEFT[place]
    : direction === "right"
      ? LINES_RIGHT[place]
      : direction === "up"
        ? LINES_UP[place]
        : LINES_DOWN[place];
}

/** Where a cell sits in a direction's list of lines. */
function place_of_cell(direction: Direction, cell: number): number {
  return direction === "left"
    ? PLACE_LEFT[cell]
    : direction === "right"
      ? PLACE_RIGHT[cell]
      : direction === "up"
        ? PLACE_UP[cell]
        : PLACE_DOWN[cell];
}

/** Tiles are stored as levels, 1 for a 2, 2 for a 4 and so on, with 0 for an empty cell. */
const TILE_VALUES = [
  0, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072,
];

/** The level at one place of a line of four after it slides towards its start, merging each pair once. */
function slid_level(board: number[], direction: Direction, start: number, wanted: number): number {
  let waiting = 0;
  let placed = 0;
  let found = 0;
  for (const place of PLACES_IN_LINE) {
    const level = board[line_cell(direction, start + place)];
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
function line_points(board: number[], direction: Direction, start: number): number {
  let waiting = 0;
  let points = 0;
  for (const place of PLACES_IN_LINE) {
    const level = board[line_cell(direction, start + place)];
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
  return range(5).map((place) => (place < 4 ? slid_level(line, "left", 0, place) : line_points(line, "left", 0)));
}

/** A cell's level after a slide, from its own line in the direction's list. */
function slid_cell(board: number[], direction: Direction, cell: number): number {
  const place = place_of_cell(direction, cell);
  const start = place - (place % 4);
  return slid_level(board, direction, start, place - start);
}

/** The points all four lines score in a slide. */
export function slide_points(board: number[], direction: Direction): number {
  let points = 0;
  for (const line of PLACES_IN_LINE) {
    points = points + line_points(board, direction, line * 4);
  }
  return points;
}

/** The board after a slide. */
export function slide(board: number[], direction: Direction): number[] {
  return CELLS.map((cell) => slid_cell(board, direction, cell));
}

/** Whether a slide moves or merges any tile. */
function slide_changes(board: number[], direction: Direction): boolean {
  let changes = false;
  for (const cell of CELLS) {
    if (slid_cell(board, direction, cell) !== board[cell]) changes = true;
  }
  return changes;
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

const EMPTY_BOARD = CELLS.map(() => 0);

export const board = persisted<number[]>("board", EMPTY_BOARD);

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

/** How many cells of the board are empty. */
function count_empty_cells(): number {
  let empty_count = 0;
  for (const cell of CELLS) {
    if (board.value[cell] === 0) empty_count = empty_count + 1;
  }
  return empty_count;
}

/** Puts a 2, or one time in ten a 4, on a randomly chosen empty cell, if there is one. */
function add_new_tile() {
  const empty_count = count_empty_cells();
  if (empty_count === 0) return;
  const chosen = random_int(0, empty_count - 1);
  const level = random_int(1, 10) === 1 ? 2 : 1;
  let empty_seen = 0;
  for (const cell of CELLS) {
    if (board.value[cell] === 0) {
      if (empty_seen === chosen) board.value[cell] = level;
      empty_seen = empty_seen + 1;
    }
  }
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
  board.value = slide(board.value, event.direction);
  add_new_tile();
  score.value = new_score;
  if (new_score > best_score.value) best_score.value = new_score;
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

/** One tile is one label, its own background drawn behind its centred number. */
function Tile(props: { cell: number }) {
  const level = board.value[props.cell];
  return (
    <Label
      background={TILE_BACKGROUNDS[level]}
      radius={6}
      padding={12}
      text_align="center"
      font={level >= 14 ? "small" : "normal"}
      color={level <= 2 ? DARK_TILE_TEXT : LIGHT_TILE_TEXT}
    >
      {level === 0 ? "" : `${TILE_VALUES[level]}`}
    </Label>
  );
}

function Twos() {
  return (
    <Screen onSwipe={move} onClick={tap}>
      <Row justify="space_between" height={28} padding={4}>
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
    if (count_empty_cells() === CELLS.length) start_game();
  },
});
