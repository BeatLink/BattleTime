import { beforeEach, describe, expect, it } from "vitest";
import { reset_persisted } from "neotime";
import app, {
  clear_text,
  CODE,
  CODE_LETTERS,
  end_letter,
  keyed_text,
  line_0,
  line_1,
  line_2,
  line_3,
  line_4,
  line_5,
  line_lengths,
  next_node,
  next_span,
  refresh_lines,
  swipe,
  text_codes,
  text_count,
} from "./app.tsx";

const LETTERS = " eish54v?3uf????2arl?????wp??j?1tndb6?x??kc??y??mgz7?q??o?8??90";

function decode(symbols: string): string {
  let node = 0;
  let span = 63;
  for (const symbol of symbols) {
    node = next_node(node, span, symbol === "-");
    span = next_span(span);
  }
  return node < 0 ? "?" : LETTERS[node];
}

/** Puts text straight into the store, as if it had been keyed. */
function set_text(text: string) {
  text_codes.value = text_codes.value.map((code, position) => (position < text.length ? text.charCodeAt(position) : code));
  text_count.value = text.length;
  refresh_lines();
}

function shown_lines(): string[] {
  return [line_0, line_1, line_2, line_3, line_4, line_5].map((line, place) =>
    String.fromCharCode(...line.value.slice(0, line_lengths.value[place])),
  );
}

function stored_text(): string {
  return String.fromCharCode(...text_codes.value.slice(0, text_count.value));
}

describe("morse", () => {
  beforeEach(() => {
    reset_persisted(app);
    clear_text();
  });

  it("decodes letters and digits", () => {
    expect(decode(".")).toBe("e");
    expect(decode("-")).toBe("t");
    expect(decode(".-")).toBe("a");
    expect(decode("...")).toBe("s");
    expect(decode("---")).toBe("o");
    expect(decode("-.-.")).toBe("c");
    expect(decode("--.-")).toBe("q");
    expect(decode(".----")).toBe("1");
    expect(decode("-----")).toBe("0");
    expect(decode("......")).toBe("?");
  });

  it("keeps the letters and their character codes in step", () => {
    expect(CODE.map((code) => String.fromCharCode(code))).toEqual(CODE_LETTERS);
  });

  it("keys dots and lines and adds the letter they make", () => {
    swipe({ direction: "down" });
    swipe({ direction: "up" });
    expect(keyed_text.value).toBe(".-");
    end_letter();
    expect(keyed_text.value).toBe("");
    end_letter();
    expect(stored_text()).toBe("a ");
  });

  it("wraps long lines, breaks at new lines and keeps the last six", () => {
    set_text(`${"e".repeat(20)}\ni`);
    expect(shown_lines().slice(3)).toEqual(["e".repeat(16), "eeee", "i"]);
  });

  it("shows blank lines and an empty text", () => {
    expect(shown_lines()).toEqual(["", "", "", "", "", ""]);
    set_text("ab\n\ncd");
    expect(shown_lines()).toEqual(["", "", "", "ab", "", "cd"]);
    set_text("a\nb\nc\nd\ne\nf\ng");
    expect(shown_lines()).toEqual(["b", "c", "d", "e", "f", "g"]);
  });

  it("drops the oldest character once the text is full", () => {
    set_text("a".repeat(95) + "b");
    swipe({ direction: "down" });
    end_letter();
    expect(stored_text()).toBe("a".repeat(94) + "be");
  });

  it("turns a second space into a new line, and deletes the last character", () => {
    set_text("ab");
    swipe({ direction: "right" });
    expect(stored_text()).toBe("ab ");
    swipe({ direction: "right" });
    expect(stored_text()).toBe("ab\n");
    swipe({ direction: "left" });
    expect(stored_text()).toBe("ab");
    set_text("");
    swipe({ direction: "left" });
    expect(stored_text()).toBe("");
  });
});
