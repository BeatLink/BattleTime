// A small turning ring in the current text colour, for work that is still going.
export function Spinner({ color }: { color?: string }) {
  return <span class="spinner" role="progressbar" aria-label="Working" style={color ? { color } : undefined} />;
}
