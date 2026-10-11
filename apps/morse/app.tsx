// Morse translator and notepad, from wasp-os's morse.
// Swipe up for a line and down for a dot, then tap to end the letter; a tap with nothing keyed adds a
// space. Swipe right for a space, twice for a new line, and left to delete. The top card shows the last
// six lines and the bottom card the dots and lines keyed so far with the letter they make. The text
// persists, and a long press asks to clear it.

import { computed, signal } from "@preact/signals";
import { define_app, persisted, type SwipeEvent } from "neotime";
import { Card, ConfirmDialog } from "neotime/components";
import { icons } from "neotime/icons";
import { Label, Row, Screen } from "neotime/widgets";

// ============================================================================
// Morse code
// ============================================================================

/**
 * The code as a binary tree laid out flat: each node is followed by the half for a dot and then the half
 * for a line, so a node's two subtrees each take half of what follows it.
 */
const CODE = [
  " ", "e", "i", "s", "h", "5", "4", "v", "?", "3", "u", "f", "?", "?", "?", "?", "2", "a", "r", "l", "?", "?",
  "?", "?", "?", "w", "p", "?", "?", "j", "?", "1", "t", "n", "d", "b", "6", "?", "x", "?", "?", "k", "c", "?",
  "?", "y", "?", "?", "m", "g", "z", "7", "?", "q", "?", "?", "o", "?", "8", "?", "?", "9", "0",
];

const SPACE = 0;
const UNKNOWN = 8;

/** A character code past the end of the tree, which starts a new line. */
const NEW_LINE = 63;

const TREE_SIZE = 63;

/** The longest run of dots and lines one letter may have. */
const LONGEST_LETTER = 16;

/** Where the tree walk lands after one more dot or line, or -1 once it falls off the tree. */
export function next_node(node: number, span: number, is_line: boolean): number {
  const remaining = span - 1;
  const half = Math.floor(remaining / 2);
  return node < 0 || remaining <= 0 ? -1 : is_line ? node + 1 + half : node + 1;
}

export function next_span(span: number): number {
  return Math.floor((span - 1) / 2);
}

// ============================================================================
// Text layout
// ============================================================================

const CAPACITY = 96;
const LINE_LENGTH = 16;

const POSITIONS = [
  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
  32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60,
  61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89,
  90, 91, 92, 93, 94, 95,
];

const EMPTY_LINES = ["", "", "", "", "", ""];

function scrolled(lines: string[], newest: string): string[] {
  return [lines[1], lines[2], lines[3], lines[4], lines[5], newest];
}

/** The last six lines of the text, wrapping long lines and breaking at each new line. */
export function layout(characters: number[], count: number): string[] {
  let lines = EMPTY_LINES;
  let current = "";
  let column = 0;
  for (const position of POSITIONS) {
    if (position < count) {
      const code = characters[position];
      if (code === NEW_LINE || column === LINE_LENGTH) {
        lines = scrolled(lines, current);
        current = "";
        column = 0;
      }
      if (code !== NEW_LINE) {
        current = current + CODE[code];
        column = column + 1;
      }
    }
  }
  return scrolled(lines, current);
}

// ============================================================================
// State
// ============================================================================

/** The text as character codes, oldest first; only the first `character_count` count. */
const characters = persisted<number[]>(
  "characters",
  POSITIONS.map(() => SPACE),
);

const character_count = persisted("character_count", 0);

const lines = signal<string[]>(EMPTY_LINES);

/** The dots and lines keyed for the letter in progress. */
const keyed = signal("");
const keyed_count = signal(0);

/** Where the keyed symbols lead in the tree, and the size of the subtree there. */
const node = signal(0);
const span = signal(TREE_SIZE);

const is_asking_to_clear = signal(false);

// ============================================================================
// Derived values
// ============================================================================

const keyed_letter = computed(() => (node.value < 0 ? "?" : CODE[node.value]));

// ============================================================================
// Handlers
// ============================================================================

function refresh_lines() {
  lines.value = layout(characters.value, character_count.value);
}

function forget_keyed() {
  keyed.value = "";
  keyed_count.value = 0;
  node.value = 0;
  span.value = TREE_SIZE;
}

/** Adds a character, dropping the oldest when the text is full. */
function append(code: number) {
  const count = character_count.value;
  if (count < CAPACITY) {
    characters.value = POSITIONS.map((position) => (position === count ? code : characters.value[position]));
    character_count.value = count + 1;
  } else {
    characters.value = POSITIONS.map((position) =>
      position < CAPACITY - 1 ? characters.value[position + 1] : code,
    );
  }
  refresh_lines();
}

function key_symbol(is_line: boolean) {
  if (keyed_count.value < LONGEST_LETTER) {
    node.value = next_node(node.value, span.value, is_line);
    span.value = next_span(span.value);
    keyed.value = keyed.value + (is_line ? "-" : ".");
    keyed_count.value = keyed_count.value + 1;
  }
}

function end_letter() {
  append(node.value < 0 ? UNKNOWN : node.value);
  forget_keyed();
}

function add_space_or_new_line() {
  const count = character_count.value;
  if (count > 0 && characters.value[count - 1] === SPACE) {
    characters.value = POSITIONS.map((position) => (position === count - 1 ? NEW_LINE : characters.value[position]));
    refresh_lines();
  } else {
    append(SPACE);
  }
}

function delete_back() {
  if (keyed_count.value > 0) {
    forget_keyed();
  } else {
    character_count.value = Math.max(0, character_count.value - 1);
    refresh_lines();
  }
}

function swipe(event: SwipeEvent) {
  if (event.direction === "up") key_symbol(true);
  else if (event.direction === "down") key_symbol(false);
  else if (event.direction === "right") add_space_or_new_line();
  else delete_back();
}

function clear_text() {
  character_count.value = 0;
  forget_keyed();
  refresh_lines();
}

// ============================================================================
// Screen
// ============================================================================

function Morse() {
  return (
    <Screen
      onSwipe={swipe}
      onClick={end_letter}
      onLongPress={() => (is_asking_to_clear.value = true)}
      padding={6}
      gap={6}
    >
      <Card grow={1} gap={0}>
        {lines.value.map((line) => (
          <Label height={23} long_mode="clip">{line}</Label>
        ))}
      </Card>
      <Card height={48}>
        <Row justify="between">
          <Label color="muted" long_mode="clip">{keyed.value}</Label>
          <Label>{keyed_letter.value}</Label>
        </Row>
      </Card>
      <ConfirmDialog open={is_asking_to_clear} question="Clear all the text?" onConfirm={clear_text} />
    </Screen>
  );
}

export default define_app({
  id: "morse",
  name: "Morse",
  icon: icons.notification,
  screens: { main: Morse },
  onOpen: refresh_lines,
});
