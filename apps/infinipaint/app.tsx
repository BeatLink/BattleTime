// Finger painting, from InfiniTime's InfiniPaint.
// Touching or dragging paints the cell under the finger on a 20 by 20 grid, and a long press changes the
// colour with a short buzz, through InfiniTime's eight. The picture persists, and a double tap asks to
// clear it. Swipes are ignored so a stroke never leaves the app; the button does that.

import { signal } from "@preact/signals";
import { define_app, persisted, range, show_toast, vibrate, type ChangeEvent, type Color } from "neotime";
import { ConfirmDialog } from "neotime/components";
import { icons } from "neotime/icons";
import { CellGrid, Screen } from "neotime/widgets";

// ============================================================================
// State
// ============================================================================

const GRID_SIZE = 20;
const CELL_SIZE = 12;

/** The canvas cells, numbered across each row from the top left. */
const CELLS = range(GRID_SIZE * GRID_SIZE);

const BRUSH_COLORS: Color[] = ["#ff00ff", "#00b000", "#ffffff", "#ff0000", "#00ffff", "#ffff00", "#0000ff", "#000000"];
const BRUSH_NAMES = ["Magenta", "Green", "White", "Red", "Cyan", "Yellow", "Blue", "Black"];

const WHITE = 2;
const BLACK = 7;

const BLANK_CANVAS = CELLS.map(() => BLACK);

/** The brush colour of every cell, as a place in the colour list. */
export const canvas = persisted<number[]>("canvas", BLANK_CANVAS);

const brush = persisted("brush", WHITE);

const is_asking_to_clear = signal(false);

// ============================================================================
// Handlers
// ============================================================================

/** Paints the cell the grid reports under the finger. */
export function paint(event: ChangeEvent<number>) {
  if (!is_asking_to_clear.value && canvas.value[event.value] !== brush.value) {
    canvas.value[event.value] = brush.value;
  }
}

function change_colour() {
  brush.value = (brush.value + 1) % BRUSH_COLORS.length;
  vibrate(35);
  show_toast(BRUSH_NAMES[brush.value]);
}

function clear_canvas() {
  canvas.value = BLANK_CANVAS;
}

function ignore_swipe() {}

// ============================================================================
// Screen
// ============================================================================

function InfiniPaint() {
  return (
    <Screen
      onLongPress={change_colour}
      onDoubleClick={() => (is_asking_to_clear.value = true)}
      onSwipe={ignore_swipe}
    >
      <CellGrid
        x={0}
        y={0}
        columns={GRID_SIZE}
        rows={GRID_SIZE}
        cell_size={CELL_SIZE}
        cells={canvas.value}
        palette={BRUSH_COLORS}
        onClick={paint}
        onDrag={paint}
      />
      <ConfirmDialog open={is_asking_to_clear} question="Clear the picture?" onConfirm={clear_canvas} />
    </Screen>
  );
}

export default define_app({
  id: "infinipaint",
  name: "Paint",
  group: "games",
  icon: icons.game,
  screens: { main: InfiniPaint },
});
