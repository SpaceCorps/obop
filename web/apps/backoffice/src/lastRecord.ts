import type { Row } from "./api.ts";

/** The row `useRecord` keeps for the record that is open. */
export type Last = { model: string; id: string; row: Row };

/**
 * What `useRecord` remembers after a render. A row seen for the open record is kept, so the record
 * panel stays mounted (with its draft) while the list around it asks again and the record is not on
 * its page for a moment. It is forgotten when
 *   - the route holds no record (the panel was closed, or a new record is being made), so a record
 *     deleted from its panel does not come back with its old data when the person goes Back;
 *   - the `get` for the record did not return it (`row === null`: gone, no access, or the request
 *     failed), so nothing old is shown for it.
 * It is not forgotten while the `get` is still on its way (`row === undefined`).
 */
export function remember(
  last: Last | null,
  now: { model: string; id: string; row: Row | null | undefined },
): Last | null {
  if (!now.id || now.id === "new") return null;
  if (now.row) return { model: now.model, id: now.id, row: now.row };
  if (now.row === null) return null;
  return last;
}

/** The remembered row, when it is the row of the record asked for. */
export function remembered(last: Last | null, model: string, id: string): Row | undefined {
  return last && last.model === model && last.id === id ? last.row : undefined;
}
