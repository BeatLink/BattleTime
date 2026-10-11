// The 15 puzzle, from wasp-os's puzzle15.
// Swipes slide a tile into the gap and a tap asks to start again, as on wasp-os; the board, the move
// count and the fewest moves to a solution persist, and a solved board says so.

import { computed, signal } from "@preact/signals";
import { define_app, persisted, random_int, vibrate_pattern, type SwipeEvent } from "neotime";
import { ConfirmDialog } from "neotime/components";
import { icons } from "neotime/icons";
import { Grid, Label, Row, Screen, Stack } from "neotime/widgets";

// ============================================================================
// Rules
// ============================================================================

const CELLS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/** The places the shuffle visits; the gap stays in the last cell. */
const SHUFFLED_PLACES = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];

/** Tiles in order with the gap, written 0, in the bottom right corner. */
export const SOLVED_BOARD = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0];

const GAP_WHEN_SOLVED = 15;

function swapped(board: number[], first: number, second: number): number[] {
  return CELLS.map((cell) => (cell === first ? board[second] : cell === second ? board[first] : board[cell]));
}

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
  let solved = true;
  for (const cell of CELLS) {
    if (board[cell] !== SOLVED_BOARD[cell]) solved = false;
  }
  return solved;
}

// ============================================================================
// State
// ============================================================================

export const board = persisted<number[]>("board", SOLVED_BOARD);

const gap = persisted("gap", GAP_WHEN_SOLVED);

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

/** Deals a random solvable board: an even shuffle of the fifteen tiles, made even by one more swap if needed. */
export function start_game() {
  board.value = SOLVED_BOARD;
  let swap_count = 0;
  for (const place of SHUFFLED_PLACES) {
    const other = random_int(place, 14);
    if (other !== place) {
      board.value = swapped(board.value, place, other);
      swap_count = swap_count + 1;
    }
  }
  if (swap_count % 2 === 1) board.value = swapped(board.value, 13, 14);
  gap.value = GAP_WHEN_SOLVED;
  move_count.value = 0;
}

function slide(event: SwipeEvent) {
  const tile = tile_moved_by(gap.value, event.direction);
  if (tile >= 0 && !solved.value && !is_asking_to_restart.value) {
    board.value = swapped(board.value, gap.value, tile);
    gap.value = tile;
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

function Tile(props: { cell: number }) {
  const tile = board.value[props.cell];
  return (
    <Stack background={tile === 0 ? "background" : "surface"} radius={8}>
      <Label font="normal">{tile === 0 ? "" : `${tile}`}</Label>
    </Stack>
  );
}

function Puzzle15() {
  return (
    <Screen onSwipe={slide} onClick={tap}>
      <Row justify="between" height={28} padding={4}>
        <Label font="small" color={solved.value ? "success" : "muted"}>
          {solved.value ? `Solved in ${move_count.value}` : `${move_count.value} moves`}
        </Label>
        <Label font="small" color="muted">
          {solved.value ? "Tap to play" : fewest_moves.value > 0 ? `Best ${fewest_moves.value}` : ""}
        </Label>
      </Row>
      <Grid columns={4} gap={4} padding={4} width={240} height={212}>
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
