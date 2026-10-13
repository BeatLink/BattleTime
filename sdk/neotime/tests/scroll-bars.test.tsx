import { cleanup, fireEvent, render } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { List, Scroll, scroll_bar_parts, scroll_pages } from "../src/widgets.tsx";

describe("scroll bar arithmetic", () => {
  it("counts pages so the last one ends with the content", () => {
    expect(scroll_pages(240, 600, 0)).toEqual({ pages: 3, current: 0 });
    expect(scroll_pages(240, 600, 240)).toEqual({ pages: 3, current: 1 });
    expect(scroll_pages(240, 600, 360)).toEqual({ pages: 3, current: 2 });
    expect(scroll_pages(240, 200, 0)).toEqual({ pages: 1, current: 0 });
  });

  it("gives a paged bar a segment per page, the page on show bright", () => {
    expect(scroll_bar_parts(240, 600, 240, true)).toEqual([
      { start: 2, length: 76, bright: false },
      { start: 82, length: 76, bright: true },
      { start: 162, length: 76, bright: false },
    ]);
  });

  it("gives a smooth bar a dim track and a thumb that runs from top to bottom", () => {
    const [track, top] = scroll_bar_parts(240, 600, 0, false);
    expect(track).toEqual({ start: 2, length: 236, bright: false });
    expect(top).toEqual({ start: 2, length: 94, bright: true });
    const bottom = scroll_bar_parts(240, 600, 360, false)[1];
    expect(bottom.start + bottom.length).toBe(238);
  });

  it("keeps a thumb long enough to see, and pages too many to mark become a thumb", () => {
    expect(scroll_bar_parts(240, 100_000, 0, false)[1].length).toBe(12);
    expect(scroll_bar_parts(240, 240 * 60, 0, true)).toHaveLength(2);
  });
});

/** Makes every element report the sizes given, as a browser that lays out would. */
function pretend_sizes(sizes: { client: number; scroll: number }) {
  const restore = [
    ["clientHeight", sizes.client],
    ["scrollHeight", sizes.scroll],
    ["clientWidth", sizes.client],
    ["scrollWidth", sizes.scroll],
  ].map(([name, value]) => {
    const spy = vi.spyOn(HTMLElement.prototype, name as "clientHeight", "get").mockReturnValue(value as number);
    return () => spy.mockRestore();
  });
  return () => restore.forEach((undo) => undo());
}

function bar_parts(container: Element): HTMLElement[] {
  return Array.from(container.querySelectorAll("[data-widget=scroll-bar-part]"));
}

describe("scrolling in the preview", () => {
  let restore = () => {};

  afterEach(() => {
    cleanup();
    restore();
  });

  it("draws no bar while the content fits", () => {
    restore = pretend_sizes({ client: 240, scroll: 240 });
    const { container } = render(<Scroll height={240}>Short</Scroll>);
    expect(bar_parts(container)).toHaveLength(0);
  });

  it("draws a bar down the edge and keeps room for it while the content is longer", () => {
    restore = pretend_sizes({ client: 240, scroll: 600 });
    const { container } = render(<Scroll height={240} padding={4}>Long</Scroll>);
    const parts = bar_parts(container);
    expect(parts).toHaveLength(2);
    expect(parts[1].style.top).toBe("2px");
    expect(parts[1].style.right).toBe("0px");
    const scroller = container.querySelector("[data-widget=scroll] > div") as HTMLElement;
    expect(scroller.style.paddingRight).toBe("8px");
  });

  it("leaves the bar out when told to", () => {
    restore = pretend_sizes({ client: 240, scroll: 600 });
    const { container } = render(<Scroll height={240} scroll_bar={false}>Long</Scroll>);
    expect(bar_parts(container)).toHaveLength(0);
  });

  it("runs a list across with its bar along the bottom, and settles a paged one on a whole page", () => {
    restore = pretend_sizes({ client: 240, scroll: 720 });
    const { container } = render(
      <List width={240} direction="row" scrolling="pages">
        Wide
      </List>,
    );
    const parts = bar_parts(container);
    expect(parts).toHaveLength(3);
    expect(parts[0].style.bottom).toBe("0px");
    expect(parts[0].style.background).toBe("rgb(160, 160, 160)");

    const scroller = container.querySelector("[data-widget=list] > div") as HTMLElement;
    expect(scroller.style.flexDirection).toBe("row");
    const scroll_to = vi.fn();
    scroller.scrollTo = scroll_to as typeof scroller.scrollTo;
    scroller.scrollLeft = 300;
    fireEvent.scroll(scroller);
    expect(bar_parts(container)[1].style.background).toBe("rgb(160, 160, 160)");
    fireEvent(scroller, new Event("scrollend"));
    expect(scroll_to).toHaveBeenCalledWith({ left: 240 });
  });
});
