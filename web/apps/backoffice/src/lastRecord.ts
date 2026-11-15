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

// ---- the trail of records (plan 2.6 item 4: "Sales › SO-00001 › Ada Example") --------------------

/** One record of the trail, as a link back to it. */
export type Crumb = { key: string; label: string; href: string };

/**
 * The records the person went through by following links inside record panels, oldest first, the
 * open one last. A record opened any other way (from a list, a search, the address bar) starts a
 * new trail; a record already on the trail cuts it back to that record.
 */
export function extendTrail(
  trail: readonly Crumb[],
  next: Crumb,
  /** The record whose panel held the link that was followed, if one was. */
  from: string | null,
): Crumb[] {
  const back = trail.findIndex((c) => c.key === next.key);
  if (back >= 0) return [...trail.slice(0, back), next];
  const at = from ? trail.findIndex((c) => c.key === from) : -1;
  if (at < 0) return [next];
  return [...trail.slice(0, at + 1), next].slice(-5);
}

/** The trail kept across panels: the panel of each record replaces the one before it. */
export const trailStore: { trail: Crumb[]; from: string | null } = { trail: [], from: null };
