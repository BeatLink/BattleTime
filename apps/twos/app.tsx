// 2048, merged from InfiniTime's Twos and wasp-os's play2048.
// InfiniTime gives the score, the one-in-ten chance of a 4 and picking only empty cells; wasp-os gives
// the classic tile colours and tap to restart with a question. The board and best score persist.

import { computed, signal } from "@preact/signals";
import {
  can_slide,
  count_of,
  define_app,
  nth_index_of,
  persisted,
  random_int,
  range,
  slide_changes,
  slide_grid,
  slide_points,
  vibrate,
  type Color,
  type SwipeEvent,
} from "neotime";
import { ConfirmDialog } from "neotime/components";
import { icons } from "neotime/icons";
import { Grid, Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// Rules
// ============================================================================

/** The sixteen cells, numbered across each row from the top left. */
export const CELLS = range(16);

/** The board is four cells wide. */
const WIDTH = 4;

/** Tiles are stored as levels, 1 for a 2, 2 for a 4 and so on, with 0 for an empty cell. */
const TILE_VALUES = [
  0, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072,
];

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

const is_game_over = computed(() => !can_slide(board.value, WIDTH));

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
  if (!slide_changes(board.value, WIDTH, event.direction)) return;
  const new_score = score.value + slide_points(board.value, WIDTH, event.direction);
  board.value = slide_grid(board.value, WIDTH, event.direction);
  add_new_tile();
  score.value = new_score;
  if (new_score > best_score.value) best_score.value = new_score;
  if (!can_slide(board.value, WIDTH)) vibrate(200);
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
      width="100%"
      height="100%"
      radius={6}
      padding={8}
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
    <Screen padding={8} gap={6} onSwipe={move} onClick={tap}>
      <Row justify="space_between" height={20}>
        <Row gap={6}>
          <Label font="small" color="muted">Score</Label>
          <Label font="small" color="warning">{`${score.value}`}</Label>
        </Row>
        <Label font="small" color={is_game_over.value ? "danger" : "muted"}>
          {is_game_over.value ? "Game over, tap" : `Best ${best_score.value}`}
        </Label>
      </Row>
      <Grid columns={4} gap={6} grow={1}>
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
    if (count_of(board.value, 0) === CELLS.length) start_game();
  },
});
