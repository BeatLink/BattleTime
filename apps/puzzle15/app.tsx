// The 15 puzzle, from wasp-os's puzzle15.
// Swipes slide a tile into the gap and a tap asks to start again, as on wasp-os; the board, the move
// count and the fewest moves to a solution persist, and a solved board says so.

import { computed, signal } from "@preact/signals";
import { define_app, persisted, random_int, range, vibrate_pattern, type SwipeEvent } from "neotime";
import { ConfirmDialog } from "neotime/components";
import { icons } from "neotime/icons";
import { Grid, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// Rules
// ============================================================================

const CELLS = range(16);

/** The places the shuffle visits; the gap stays in the last cell. */
const SHUFFLED_PLACES = range(15);

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

/** Deals a random solvable board: an even shuffle of the fifteen tiles, made even by one more swap if needed. */
export function start_game() {
  board.value = SOLVED_BOARD;
  let swap_count = 0;
  for (const place of SHUFFLED_PLACES) {
    const other = random_int(place, 14);
    if (other !== place) {
      swap_cells(place, other);
      swap_count = swap_count + 1;
    }
  }
  if (swap_count % 2 === 1) swap_cells(13, 14);
  move_count.value = 0;
}

/** The cell of the gap, which is wherever the board holds 0. */
function gap_cell(): number {
  let gap = 0;
  for (const cell of CELLS) {
    if (board.value[cell] === 0) gap = cell;
  }
  return gap;
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
function Tile(props: { cell: number }) {
  const tile = board.value[props.cell];
  return (
    <Label width="100%" height="100%" background={tile === 0 ? "background" : "surface"} radius={8} padding={8} text_align="center" font="normal">
      {tile === 0 ? "" : `${tile}`}
    </Label>
  );
}

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
      <Grid columns={4} gap={6} grow={1}>
        {CELLS.map((cell) => (
          <Tile cell={cell} />
        ))}
      </Grid>
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
  icon: icons.game,
  screens: { main: Puzzle15 },
  onOpen: () => {
    if (move_count.value === 0 && is_solved(board.value)) start_game();
  },
});
