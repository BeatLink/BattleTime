// Conway's Game of Life, from wasp-os's game_of_life.
// A cell lives on with two or three live neighbours and is born with exactly three. It starts from an
// acorn, and every generation is drawn in a new colour, as on wasp-os. A tap starts the acorn again and
// a long press scatters a random soup. The 16 by 16 board wraps at its edges.

import { signal } from "@preact/signals";
import { define_app, every, keep_screen_on, life_step, random_int, range, type Color } from "neotime";
import { icons } from "neotime/icons";
import { CellGrid, Screen } from "neotime/widgets";

// ============================================================================
// Rules
// ============================================================================

const BOARD_SIZE = 16;
const CELL_SIZE = 15;

/** The board's cells, numbered across each row from the top left. */
const CELLS = range(BOARD_SIZE * BOARD_SIZE);

/** How many colours the generations cycle through. */
const COLOR_COUNT = 9;

/** A small pattern that grows for thousands of generations on an open board. */
const ACORN_CELLS = [118, 136, 149, 150, 153, 154, 155];

/** Each cell holds 0 when dead, or the place of its generation's colour in the palette when alive. */
export const ACORN = CELLS.map((cell) => (ACORN_CELLS.includes(cell) ? 1 : 0));

/** The next generation, with every live cell holding `live_value`, on a board that wraps at its edges. */
export function next_generation(board: number[], live_value: number): number[] {
  return life_step(board, BOARD_SIZE, live_value);
}

// ============================================================================
// State
// ============================================================================

/** Dead cells, then the bright colours the generations cycle through. */
const PALETTE: Color[] = [
  "background", "#ff4040", "#ffa020", "#f0f040", "#60ff40", "#40ffc0", "#40c0ff", "#6060ff", "#c040ff", "#ff40a0",
];

const board = signal<number[]>(ACORN);

const generation = signal(0);

// ============================================================================
// Handlers
// ============================================================================

/** Every cell reads its neighbours on the board as it was, because a whole-array write reads the old array. */
function step() {
  const next = generation.value + 1;
  board.value = next_generation(board.value, 1 + (next % COLOR_COUNT));
  generation.value = next;
}

function plant_acorn() {
  board.value = ACORN;
  generation.value = 0;
}

function scatter_soup() {
  board.value = CELLS.map(() => (random_int(1, 3) === 1 ? 1 : 0));
  generation.value = 0;
}

// ============================================================================
// Screen
// ============================================================================

function GameOfLife() {
  return (
    <Screen onClick={plant_acorn} onLongPress={scatter_soup}>
      <CellGrid
        x={0}
        y={0}
        columns={BOARD_SIZE}
        rows={BOARD_SIZE}
        cell_size={CELL_SIZE}
        cells={board.value}
        palette={PALETTE}
      />
    </Screen>
  );
}

export default define_app({
  id: "game-of-life",
  name: "Life",
  group: "games",
  icon: icons.game,
  screens: { main: GameOfLife },
  subscriptions: [every({ milliseconds: 625 }, step, { only_while_open: true })],
  onOpen: () => keep_screen_on(true),
});
