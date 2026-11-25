/**
 * How the room of the home screen is shared between the round button (under the current widget)
 * and Recent trips (under it, down to the bottom of the screen) (pure, so it is tested): the
 * recent trips take as many rows as fit, up to `maxRows`, with the round button at least
 * `minButton`; the round button takes the rest, up to `maxButton`, so the list ends one padding
 * of the screen above the switcher. All in the units of the screen (dp).
 */
export function shareRoom(
  /**
   * The height of the round button and the rows of the list together: from the top of the list
   * to the bottom of the screen's content, and the round button as it is now.
   */
  room: number,
  rowHeight: number,
  limits: { maxRows: number; minButton: number; maxButton: number },
): { rows: number; button: number } {
  const fit = Math.floor((room - limits.minButton) / rowHeight + 1e-6);
  const rows = Math.max(1, Math.min(limits.maxRows, fit));
  // The room does not depend on the button (a bigger button moves the list down as much), so the
  // rest can be given whole, down to the point.
  const rest = Math.floor(room - rows * rowHeight);
  return { rows, button: Math.max(limits.minButton, Math.min(limits.maxButton, rest)) };
}
