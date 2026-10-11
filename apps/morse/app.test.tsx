import { describe, expect, it } from "vitest";
import { layout, next_node, next_span } from "./app.tsx";

const CODE = " eish54v?3uf????2arl?????wp??j?1tndb6?x??kc??y??mgz7?q??o?8??90";

function decode(symbols: string): string {
  let node = 0;
  let span = 63;
  for (const symbol of symbols) {
    node = next_node(node, span, symbol === "-");
    span = next_span(span);
  }
  return node < 0 ? "?" : CODE[node];
}

describe("morse", () => {
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

  it("wraps long lines, breaks at new lines and keeps the last six", () => {
    const text = [...Array(20).fill(1), 63, 2];
    const lines = layout([...text, ...Array(96 - text.length).fill(0)], text.length);
    expect(lines.slice(3)).toEqual(["e".repeat(16), "eeee", "i"]);
  });
});
