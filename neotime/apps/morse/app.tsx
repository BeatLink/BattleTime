// Morse translator and notepad, from wasp-os's morse.
// Swipe up for a line and down for a dot, then tap to end the letter; a tap with nothing keyed adds a
// space. Swipe right for a space, twice for a new line, and left to delete. The top card shows as many
// of the last lines as fit and the bottom card the dots and lines keyed so far with the letter they make. The text
// persists, and a long press asks to clear it.

import { computed, define_app, persisted, push_shift, range, signal, text_from_codes, type SwipeEvent } from "neotime";
import { Card, ConfirmDialog } from "neotime/components";
import { icons } from "neotime/icons";
import { Label, Row, Screen, TextLines } from "neotime/widgets";

// ============================================================================
// Morse code
// ============================================================================

/**
 * The code as a binary tree laid out flat: each node is followed by the half for a dot and then the half
 * for a line, so a node's two subtrees each take half of what follows it.
 */
export const CODE_LETTERS = [
  " ", "e", "i", "s", "h", "5", "4", "v", "?", "3", "u", "f", "?", "?", "?", "?", "2", "a", "r", "l", "?", "?",
  "?", "?", "?", "w", "p", "?", "?", "j", "?", "1", "t", "n", "d", "b", "6", "?", "x", "?", "?", "k", "c", "?",
  "?", "y", "?", "?", "m", "g", "z", "7", "?", "q", "?", "?", "o", "?", "8", "?", "?", "9", "0",
];

/** The same tree as character codes, which is how the text is kept. */
export const CODE = [
  32, 101, 105, 115, 104, 53, 52, 118, 63, 51, 117, 102, 63, 63, 63, 63, 50, 97, 114, 108, 63, 63, 63, 63, 63, 119,
  112, 63, 63, 106, 63, 49, 116, 110, 100, 98, 54, 63, 120, 63, 63, 107, 99, 63, 63, 121, 63, 63, 109, 103, 122, 55,
  63, 113, 63, 63, 111, 63, 56, 63, 63, 57, 48,
];

const UNKNOWN_LETTER = "?";
const UNKNOWN = 63;

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
// State
// ============================================================================

/** The most characters the text keeps; adding one more drops the oldest. */
const CAPACITY = 96;

const POSITIONS = range(CAPACITY);
const KEYED_PLACES = range(LONGEST_LETTER);

/** Character codes for a new line, a space, and the dot and line keyed. */
const NEW_LINE = 10;
const SPACE = 32;
const DOT = 46;
const DASH = 45;

/** The text as character codes, oldest first; only the first `text_count` count. */
export const text_codes = persisted<number[]>(
  "text_codes",
  POSITIONS.map(() => SPACE),
);
export const text_count = persisted("text_count", 0);

/** The dots and lines keyed for the letter in progress, as character codes. */
const keyed_codes = signal<number[]>(KEYED_PLACES.map(() => DOT));
const keyed_count = signal(0);

/** Where the keyed symbols lead in the tree, and the size of the subtree there. */
const node = signal(0);
const span = signal(TREE_SIZE);

const is_asking_to_clear = signal(false);

// ============================================================================
// Derived values
// ============================================================================

export const keyed_text = computed(() => text_from_codes(keyed_codes.value, keyed_count.value));

const keyed_letter = computed(() => (node.value < 0 ? UNKNOWN_LETTER : CODE_LETTERS[node.value]));

// ============================================================================
// Handlers
// ============================================================================

function forget_keyed() {
  keyed_count.value = 0;
  node.value = 0;
  span.value = TREE_SIZE;
}

/** Adds a character, dropping the oldest when the text is full. */
function append(code: number) {
  if (text_count.value < CAPACITY) {
    text_codes.value[text_count.value] = code;
    text_count.value = text_count.value + 1;
  } else {
    text_codes.value = push_shift(text_codes.value, code);
  }
}

function key_symbol(is_line: boolean) {
  if (keyed_count.value < LONGEST_LETTER) {
    node.value = next_node(node.value, span.value, is_line);
    span.value = next_span(span.value);
    keyed_codes.value[keyed_count.value] = is_line ? DASH : DOT;
    keyed_count.value = keyed_count.value + 1;
  }
}

export function end_letter() {
  append(node.value < 0 ? UNKNOWN : CODE[node.value]);
  forget_keyed();
}

/** Adds a space, or turns a space just before into a new line. */
function add_space_or_new_line() {
  if (text_count.value > 0 && text_codes.value[text_count.value - 1] === SPACE) {
    text_codes.value[text_count.value - 1] = NEW_LINE;
  } else {
    append(SPACE);
  }
}

function delete_back() {
  if (keyed_count.value > 0) {
    forget_keyed();
  } else {
    text_count.value = Math.max(0, text_count.value - 1);
    }
}

export function swipe(event: SwipeEvent) {
  if (event.direction === "up") key_symbol(true);
  else if (event.direction === "down") key_symbol(false);
  else if (event.direction === "right") add_space_or_new_line();
  else delete_back();
}

export function clear_text() {
  text_count.value = 0;
  forget_keyed();
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
      padding={8}
      gap={6}
    >
      <Card grow={1} gap={0}>
        <TextLines grow={1} codes={text_codes.value} length={text_count.value} />
      </Card>
      <Card height={48}>
        <Row justify="space_between">
          <Label color="muted" long_mode="clip">{keyed_text.value}</Label>
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
  group: "tools",
  icon: icons.notification,
  screens: { main: Morse },
});
