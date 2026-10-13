import { act, cleanup, render } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Label, Page, Pages, Screen, swipe } from "../src/widgets.tsx";

/** Which page is on show, and how many segments the page bar has. */
function shown(container: Element): { page: string | null; segments: number } {
  return {
    page: container.querySelector("[data-widget=pages]")?.getAttribute("data-page") ?? null,
    segments: container.querySelectorAll("[data-widget=page-bar-part]").length,
  };
}

describe("Pages in the preview", () => {
  afterEach(cleanup);

  it("turns a page for a swipe across and leaves swipes past the ends to the screen", () => {
    const on_swipe = vi.fn();
    const on_page_change = vi.fn();
    const { container } = render(
      <Screen onSwipe={on_swipe}>
        <Pages onPageChange={on_page_change}>
          <Page>
            <Label>One</Label>
          </Page>
          <Page scrolls>
            <Label>Two</Label>
          </Page>
        </Pages>
      </Screen>,
    );
    expect(shown(container)).toEqual({ page: "0", segments: 2 });

    act(() => swipe("right"));
    act(() => swipe("left"));
    expect(shown(container).page).toBe("1");
    expect(container.textContent).toBe("Two");
    expect(container.querySelector("[data-widget=scroll]")).toBeTruthy();
    act(() => swipe("left"));
    act(() => swipe("up"));

    expect(on_page_change.mock.calls).toEqual([[{ page: 1 }]]);
    expect(on_swipe.mock.calls.map(([event]) => event.direction)).toEqual(["right", "left", "up"]);
  });

  it("opens on its value, keeps it within the pages, and shows no bar for one page", () => {
    const { container, rerender } = render(
      <Pages value={5}>
        <Page />
        <Page />
      </Pages>,
    );
    expect(shown(container)).toEqual({ page: "1", segments: 2 });

    rerender(
      <Pages value={0}>
        <Page />
      </Pages>,
    );
    expect(shown(container)).toEqual({ page: "0", segments: 0 });
  });
});
