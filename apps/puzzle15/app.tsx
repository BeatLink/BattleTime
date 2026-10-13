// The 15 puzzle, from wasp-os's puzzle15.
// Swipes slide a tile into the gap and a tap asks to start again, as on wasp-os; the board, the move
// count and the fewest moves to a solution persist, and a solved board says so.

import { computed, signal } from "@preact/signals";
import { count_inversions, define_app, persisted, range, shuffled, vibrate_pattern, type Color, type SwipeEvent } from "neotime";
import { ConfirmDialog } from "neotime/components";
import { icons } from "neotime/icons";
import { CellGrid, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// Rules
// ============================================================================

const CELLS = range(16);

/** Each tile's side and the space between tiles, in pixels, and the whole board's side. */
const CELL_SIZE = 45;
const CELL_GAP = 6;
const BOARD_SIDE = 4 * CELL_SIZE + 3 * CELL_GAP;

/** The gap, 0, shows the background, and every tile the surface colour. */
const TILE_COLORS: Color[] = CELLS.map((tile) => (tile === 0 ? "background" : "surface"));

/** Tiles in order with the gap, written 0, in the bottom right corner. */
export const SOLVED_BOARD = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0];

/** The cell whose tile a swipe moves into the gap, or -1 if the gap is against that edge. */
export function tile_moved_by(gap: number, direction: SwipeEvent["direction"]): number {
  const column = gap % 4;
  return direction === "left"
    ? column < 3 ? gap + 1 : -1
    : direction === "right"
      ? column > 0 ? gap - 1 : -1
      : direction === "up"
        ? gap < 12 ? gap + 4 : -1
        : gap >= 4 ? gap - 4 : -1;
}

export function is_solved(board: number[]): boolean {
  let is_in_order = true;
  for (const cell of CELLS) {
    if (board[cell] !== SOLVED_BOARD[cell]) is_in_order = false;
  }
  return is_in_order;
}

// ============================================================================
// State
// ============================================================================

export const board = persisted<number[]>("board", SOLVED_BOARD);

const move_count = persisted("move_count", 0);

/** The fewest moves any solution has taken, or 0 before the first. */
const fewest_moves = persisted("fewest_moves", 0);

const is_asking_to_restart = signal(false);

// ============================================================================
// Derived values
// ============================================================================

const solved = computed(() => is_solved(board.value));

// ============================================================================
// Handlers
// ============================================================================

/** Swaps the tiles in two cells of the board. */
function swap_cells(first: number, second: number) {
  const first_tile = board.value[first];
  board.value[first] = board.value[second];
  board.value[second] = first_tile;
}

/** Deals a random solvable board: the fifteen tiles shuffled, made an even order by one more swap if needed. */
export function start_game() {
  board.value = SOLVED_BOARD;
  board.value = shuffled(board.value, 0, 15);
  // With the gap last, a board can be solved when the tiles' own order is even; the gap adds 15 inversions.
  if (count_inversions(board.value) % 2 === 0) swap_cells(13, 14);
  move_count.value = 0;
}

/** The cell of the gap, which is wherever the board holds 0. */
function gap_cell(): number {
  return board.value.indexOf(0);
}

/** Moves the tile beside the gap into it. */
export function slide(event: SwipeEvent) {
  const gap = gap_cell();
  const tile = tile_moved_by(gap, event.direction);
  if (tile >= 0 && !solved.value && !is_asking_to_restart.value) {
    swap_cells(gap, tile);
    move_count.value = move_count.value + 1;
    if (is_solved(board.value)) {
      vibrate_pattern("success");
      if (fewest_moves.value === 0 || move_count.value < fewest_moves.value) fewest_moves.value = move_count.value;
    }
  }
}

function tap() {
  if (solved.value) start_game();
  else is_asking_to_restart.value = true;
}

// ============================================================================
// Screen
// ============================================================================

/** One tile is one label, its own background drawn behind its centred number. */
function Puzzle15() {
  return (
    <Screen padding={8} gap={6} onSwipe={slide} onClick={tap}>
      <Row justify="space_between" height={20}>
        <Label font="small" color={solved.value ? "success" : "muted"}>
          {solved.value ? `Solved in ${move_count.value}` : `${move_count.value} moves`}
        </Label>
        <Label font="small" color="muted">
          {solved.value ? "Tap to play" : fewest_moves.value > 0 ? `Best ${fewest_moves.value}` : ""}
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
          palette={TILE_COLORS}
          show_numbers
        />
      </Row>
      <ConfirmDialog
        open={is_asking_to_restart}
        question={`${move_count.value} moves. Again?`}
        onConfirm={start_game}
      />
    </Screen>
  );
}

export default define_app({
  id: "puzzle15",
  name: "15",
  group: "games",
  icon: icons.game,
  screens: { main: Puzzle15 },
  onOpen: () => {
    if (move_count.value === 0 && is_solved(board.value)) start_game();
  },
});
