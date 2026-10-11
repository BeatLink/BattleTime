// Conway's Game of Life, from wasp-os's game_of_life.
// A cell lives on with two or three live neighbours and is born with exactly three. It starts from an
// acorn, and every generation is drawn in a new colour, as on wasp-os. A tap starts the acorn again and
// a long press scatters a random soup. The 16 by 16 board wraps at its edges.

import { signal } from "@preact/signals";
import { define_app, every, keep_screen_on, random_int, type Color } from "neotime";
import { icons } from "neotime/icons";
import { Column, Screen } from "neotime/widgets";

// ============================================================================
// Rules
// ============================================================================

const BOARD_SIZE = 16;
const CELL_SIZE = 15;

/** The board's cells, numbered across each row from the top left. */
const CELLS = [
  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
  16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
  32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
  48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63,
  64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79,
  80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95,
  96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111,
  112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127,
  128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143,
  144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159,
  160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175,
  176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191,
  192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207,
  208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223,
  224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239,
  240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255,
];

/** A small pattern that grows for thousands of generations on an open board. */
const ACORN_CELLS = [118, 136, 149, 150, 153, 154, 155];

function is_acorn_cell(cell: number): boolean {
  let found = false;
  for (const acorn_cell of ACORN_CELLS) {
    if (acorn_cell === cell) found = true;
  }
  return found;
}

export const ACORN = CELLS.map((cell) => (is_acorn_cell(cell) ? 1 : 0));

/** Whether a cell is alive in the next generation; 1 for alive, 0 for dead. */
function next_state(board: number[], cell: number): number {
  const row = Math.floor(cell / BOARD_SIZE);
  const column = cell % BOARD_SIZE;
  const row_above = ((row + BOARD_SIZE - 1) % BOARD_SIZE) * BOARD_SIZE;
  const this_row = row * BOARD_SIZE;
  const row_below = ((row + 1) % BOARD_SIZE) * BOARD_SIZE;
  const left = (column + BOARD_SIZE - 1) % BOARD_SIZE;
  const right = (column + 1) % BOARD_SIZE;
  const neighbours =
    board[row_above + left] + board[row_above + column] + board[row_above + right] +
    board[this_row + left] + board[this_row + right] +
    board[row_below + left] + board[row_below + column] + board[row_below + right];
  return neighbours === 3 || (board[cell] === 1 && neighbours === 2) ? 1 : 0;
}

export function next_generation(board: number[]): number[] {
  return CELLS.map((cell) => next_state(board, cell));
}

// ============================================================================
// State
// ============================================================================

/** Bright colours the generations cycle through. */
const GENERATION_COLORS: Color[] = [
  "#ff4040", "#ffa020", "#f0f040", "#60ff40", "#40ffc0", "#40c0ff", "#6060ff", "#c040ff", "#ff40a0",
];

const board = signal<number[]>(ACORN);

const generation = signal(0);

// ============================================================================
// Handlers
// ============================================================================

function step() {
  board.value = next_generation(board.value);
  generation.value = generation.value + 1;
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
  const color = GENERATION_COLORS[generation.value % GENERATION_COLORS.length];
  return (
    <Screen onClick={plant_acorn} onLongPress={scatter_soup}>
      {CELLS.map(
        (cell) =>
          board.value[cell] === 1 && (
            <Column
              x={(cell % BOARD_SIZE) * CELL_SIZE + 1}
              y={Math.floor(cell / BOARD_SIZE) * CELL_SIZE + 1}
              width={CELL_SIZE - 2}
              height={CELL_SIZE - 2}
              radius={3}
              background={color}
            />
          ),
      )}
    </Screen>
  );
}

export default define_app({
  id: "game-of-life",
  name: "Life",
  icon: icons.game,
  screens: { main: GameOfLife },
  subscriptions: [every({ milliseconds: 625 }, step)],
  onOpen: () => keep_screen_on(true),
});
