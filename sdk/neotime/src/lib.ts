// Common functions kept in the shared library as bytecode, so apps call them instead of copying them.
// Numbers on the watch are whole, so angles come back in thousandths.

// ============================================================================
// Angles
// ============================================================================

/** The sine of an angle in degrees, in thousandths, read from a table on the watch. */
export function sine(degrees: number): number {
  return Math.round(Math.sin((degrees * Math.PI) / 180) * 1000);
}

/** The cosine of an angle in degrees, in thousandths. */
export function cosine(degrees: number): number {
  return Math.round(Math.cos((degrees * Math.PI) / 180) * 1000);
}

/** The point at a distance from a centre, at an angle in degrees clockwise from twelve o'clock. */
export function point_on_circle(
  center_x: number,
  center_y: number,
  radius: number,
  degrees: number,
): { x: number; y: number } {
  return {
    x: center_x + Math.round((radius * sine(degrees)) / 1000),
    y: center_y - Math.round((radius * cosine(degrees)) / 1000),
  };
}
