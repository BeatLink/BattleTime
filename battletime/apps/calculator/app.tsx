// Calculator, merged from wasp-os's calculator and InfiniTime's Calculator.
// From InfiniTime: fixed-point sums of up to fifteen digits, the running result above the entry, a
// sign key, repeating `=`, and plain messages for overflow and division by zero.
// From wasp-os: a key for each operator, a clear key, and keeping the sum when the app is closed.
// Numbers are whole on the watch, so every value is held in millionths.

import { computed, define_app, format_fixed, persisted, range, vibrate, type ChangeEvent } from "battletime";
import { icons } from "battletime/icons";
import { ButtonMatrix, Column, Label, Screen } from "battletime/widgets";

// ============================================================================
// State
// ============================================================================

const MILLIONTHS = 1000000;

/** The largest value held, 999999999.999999: fifteen digits, which stay exact in the browser preview too. */
const LARGEST_VALUE = 999999999999999;
const LARGEST_WHOLE_PART = 999999999;

const DECIMAL_COUNT = 6;

/** The value of a digit typed at each decimal place, in millionths. */
const DECIMAL_PLACE_VALUES = [100000, 10000, 1000, 100, 10, 1];

const KEYPAD_ROWS = [
  ["7", "8", "9", "←", "C"],
  ["4", "5", "6", "+", "-"],
  ["1", "2", "3", "×", "÷"],
  ["0", ".", "±", "="],
];

/** What each key does, in the order the keypad reports them. */
const KEY_ACTIONS = [
  "digit", "digit", "digit", "back", "clear",
  "digit", "digit", "digit", "+", "-",
  "digit", "digit", "digit", "*", "/",
  "digit", ".", "sign", "=",
];

/** The digit on each key, or -1 for keys that are not digits. */
const KEY_DIGITS = [7, 8, 9, -1, -1, 4, 5, 6, -1, -1, 1, 2, 3, -1, -1, 0, -1, -1, -1];

type Operation = " " | "+" | "-" | "*" | "/";
type Problem = "" | "too large" | "zero division";

/** The number being typed, without its sign, in millionths. */
const entry = persisted("entry", 0);
const entry_is_negative = persisted("entry_is_negative", false);
const entry_has_point = persisted("entry_has_point", false);
const decimals_typed = persisted("decimals_typed", 0);

/** The result so far, in millionths. */
const result = persisted("result", 0);
const operation = persisted<Operation>("operation", " ");
const problem = persisted<Problem>("problem", "");
const just_evaluated = persisted("just_evaluated", false);

// ============================================================================
// Formatting
// ============================================================================

/** A value in millionths with its trailing zero decimals left off. */
function format_result(value: number): string {
  return format_fixed(value, DECIMAL_COUNT, true);
}

/** The entry as typed, keeping a trailing point or zeros. */
function format_entry(magnitude: number, is_negative: boolean, has_point: boolean, decimals: number): string {
  const typed = Math.floor(magnitude / (decimals === 0 ? MILLIONTHS : DECIMAL_PLACE_VALUES[decimals - 1]));
  return `${is_negative ? "-" : ""}${format_fixed(typed, decimals)}${has_point && decimals === 0 ? "." : ""}`;
}

// ============================================================================
// Derived values
// ============================================================================

const operation_symbol = computed(() =>
  operation.value === "*" ? "×" : operation.value === "/" ? "÷" : operation.value === " " ? "" : operation.value,
);

export const result_text = computed(() => `${format_result(result.value)} ${operation_symbol.value}`);

export const entry_text = computed(() =>
  problem.value !== ""
    ? problem.value
    : format_entry(entry.value, entry_is_negative.value, entry_has_point.value, decimals_typed.value),
);

// ============================================================================
// Arithmetic
// ============================================================================

/** The product of two values in millionths, or -1 if it is too large; both must be positive. */
function multiply(left: number, right: number): number {
  const left_whole = Math.floor(left / MILLIONTHS);
  const left_fraction = left % MILLIONTHS;
  const right_whole = Math.floor(right / MILLIONTHS);
  const right_fraction = right % MILLIONTHS;
  if (right_whole > 0 && left_whole > Math.floor(LARGEST_WHOLE_PART / right_whole)) return -1;

  const product =
    left_whole * right_whole * MILLIONTHS +
    left_whole * right_fraction +
    left_fraction * right_whole +
    Math.floor((left_fraction * right_fraction) / MILLIONTHS);
  return product > LARGEST_VALUE ? -1 : product;
}

/** The quotient of two values in millionths, cut to six decimals, or -1 if it is too large. */
function divide(dividend: number, divisor: number): number {
  let quotient = Math.floor(dividend / divisor);
  if (quotient > LARGEST_WHOLE_PART) return -1;

  let remainder = dividend % divisor;
  for (const _place of range(DECIMAL_COUNT)) {
    remainder = remainder * 10;
    quotient = quotient * 10 + Math.floor(remainder / divisor);
    remainder = remainder % divisor;
  }
  return quotient;
}

/** Applies the waiting operation to the result and the entry. */
function evaluate() {
  const value = entry_is_negative.value ? -entry.value : entry.value;
  const left = result.value;
  let next = left;
  if (operation.value === " ") {
    next = value;
  } else if (operation.value === "+") {
    next = left + value;
  } else if (operation.value === "-") {
    next = left - value;
  } else if (operation.value === "*") {
    const magnitude = multiply(Math.abs(left), Math.abs(value));
    next = magnitude < 0 ? LARGEST_VALUE + 1 : (left < 0) !== (value < 0) ? -magnitude : magnitude;
  } else if (value === 0) {
    problem.value = "zero division";
    return;
  } else {
    const magnitude = divide(Math.abs(left), Math.abs(value));
    next = magnitude < 0 ? LARGEST_VALUE + 1 : (left < 0) !== (value < 0) ? -magnitude : magnitude;
  }
  if (Math.abs(next) > LARGEST_VALUE) {
    problem.value = "too large";
    return;
  }
  result.value = next;
}

// ============================================================================
// Handlers
// ============================================================================

function reset_entry() {
  entry.value = 0;
  entry_is_negative.value = false;
  entry_has_point.value = false;
  decimals_typed.value = 0;
  operation.value = " ";
  problem.value = "";
  just_evaluated.value = false;
}

function type_digit(digit: number) {
  if (entry_has_point.value) {
    if (decimals_typed.value < DECIMAL_COUNT) {
      entry.value = entry.value + digit * DECIMAL_PLACE_VALUES[decimals_typed.value];
      decimals_typed.value = decimals_typed.value + 1;
    }
  } else if (entry.value <= Math.floor((LARGEST_VALUE - digit * MILLIONTHS) / 10)) {
    entry.value = entry.value * 10 + digit * MILLIONTHS;
  }
}

function delete_last() {
  if (decimals_typed.value > 0) {
    const place_value = DECIMAL_PLACE_VALUES[decimals_typed.value - 1];
    entry.value = entry.value - (Math.floor(entry.value / place_value) % 10) * place_value;
    decimals_typed.value = decimals_typed.value - 1;
  } else if (entry_has_point.value) {
    entry_has_point.value = false;
  } else if (entry.value !== 0) {
    entry.value = Math.floor(entry.value / (10 * MILLIONTHS)) * MILLIONTHS;
  } else {
    result.value = 0;
    operation.value = " ";
  }
}

function choose_operation(next: Operation) {
  if (entry.value !== 0 || entry_has_point.value) {
    evaluate();
    const failed = problem.value !== "";
    entry.value = 0;
    entry_is_negative.value = false;
    entry_has_point.value = false;
    decimals_typed.value = 0;
    if (failed) return;
  }
  operation.value = next;
}

export function press_key(event: ChangeEvent<number>) {
  const action = KEY_ACTIONS[event.value];
  if (problem.value !== "" || (just_evaluated.value && action !== "=")) {
    reset_entry();
  }

  if (action === "clear") {
    reset_entry();
    result.value = 0;
  } else if (action === "back") {
    delete_last();
  } else if (action === "sign") {
    entry_is_negative.value = !entry_is_negative.value;
  } else if (action === ".") {
    entry_has_point.value = true;
  } else if (action === "+" || action === "-" || action === "*" || action === "/") {
    choose_operation(action);
  } else if (action === "=") {
    just_evaluated.value = true;
    evaluate();
    if (operation.value === " ") {
      entry.value = 0;
      entry_is_negative.value = false;
      entry_has_point.value = false;
      decimals_typed.value = 0;
    }
  } else {
    type_digit(KEY_DIGITS[event.value]);
  }

  if (problem.value !== "") vibrate(100);
}

// ============================================================================
// Screen
// ============================================================================

function Calculator() {
  return (
    <Screen padding={8} gap={6}>
      <Column height={52} padding={4} justify="center">
        <Label font="small" color="muted" text_align="right" long_mode="clip">
          {result_text.value}
        </Label>
        <Label text_align="right" long_mode="clip" color={problem.value !== "" ? "danger" : "text"}>
          {entry_text.value}
        </Label>
      </Column>
      <ButtonMatrix height={166} rows={KEYPAD_ROWS} highlighted={[operation_symbol.value]} onClick={press_key} />
    </Screen>
  );
}

export default define_app({
  id: "calculator",
  name: "Calculator",
  group: "tools",
  icon: icons.calculator,
  screens: { main: Calculator },
});
