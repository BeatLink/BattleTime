import { cleanup, fireEvent, render } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CellGrid, cell_grid_layout } from "../src/widgets.tsx";

afterEach(cleanup);

/** Makes the grid report its own size on screen, as a browser that lays out would. */
function pretend_bounds(element: Element, width: number, height: number) {
  vi.spyOn(element, "getBoundingClientRect").mockReturnValue({
    left: 0,
    top: 0,
    width,
    height,
    right: width,
    bottom: height,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
}

describe("CellGrid in the preview", () => {
  it("spaces cells cell_size plus cell_gap apart, as the watch does", () => {
    expect(cell_grid_layout({ columns: 7, rows: 6, cell_size: 30, cell_gap: 4 })).toEqual({
      pitch: 34,
      width: 234,
      height: 200,
    });
    expect(cell_grid_layout({ columns: 3, rows: 2, cell_size: 10 })).toEqual({ pitch: 10, width: 30, height: 20 });
  });

  it("draws every cell at its full size, one pitch after the last", () => {
    const { container } = render(
      <CellGrid columns={2} rows={2} cell_size={30} cell_gap={4} cells={[0, 1, 0, 1]} palette={["background", "surface"]} />,
    );
    const cells = Array.from(container.querySelectorAll("[data-widget=cell]"));
    const grid = container.querySelector("[data-widget=cell-grid]")!;

    expect(grid.getAttribute("width")).toBe("64");
    expect(cells.map((cell) => [cell.getAttribute("x"), cell.getAttribute("y"), cell.getAttribute("width")])).toEqual([
      ["0", "0", "30"],
      ["34", "0", "30"],
      ["0", "34", "30"],
      ["34", "34", "30"],
    ]);
  });

  it("sends the cell under a tap, a gap counting as the cell before it", () => {
    const tapped: number[] = [];
    const { container } = render(
      <CellGrid
        columns={2}
        rows={2}
        cell_size={30}
        cell_gap={4}
        cells={[0, 0, 0, 0]}
        palette={["background"]}
        onClick={(event) => tapped.push(event.value)}
      />,
    );
    const grid = container.querySelector("[data-widget=cell-grid]")!;
    pretend_bounds(grid, 64, 64);

    fireEvent.click(grid, { clientX: 36, clientY: 10 });
    fireEvent.click(grid, { clientX: 32, clientY: 40 });
    fireEvent.click(grid, { clientX: 70, clientY: 10 });

    expect(tapped).toEqual([1, 2]);
  });
});
