// How a picker finds the record a typed name means before it makes one (a vendor is its contact:
// typing a vendor's name finds or makes the contact first). Pure, so it is tested without a
// server. RefPicker.tsx asks the server with `searchWord` and decides with `byName` and `existing`.
import type { Row } from "../api.ts";

/** A name as a person means it: no case, no space at either end, one space between words. */
export function nameKey(s: string): string {
  return s.trim().replace(/\s+/g, " ").toLowerCase();
}

/**
 * The word to ask the server for (`contains`, which ignores case): the longest word of the name,
 * so repeated spaces or another case still find the record.
 */
export function searchWord(text: string): string {
  return nameKey(text)
    .split(" ")
    .reduce((a, b) => (b.length > a.length ? b : a), "");
}

/**
 * What a typed name finds among the rows the server gave: exactly one record of that name, several
 * (a person must choose), or none. `complete` is false when the server may have held some back
 * (it gave as many rows as were asked for): then no match is not proof, and a person chooses.
 */
export function byName(
  rows: readonly Row[],
  field: string,
  text: string,
  complete: boolean,
): { kind: "one"; id: string } | { kind: "several" } | { kind: "none" } {
  const key = nameKey(text);
  const hits = rows.filter((r) => typeof r[field] === "string" && nameKey(r[field]) === key);
  if (hits.length === 1) return { kind: "one", id: hits[0]!.id };
  if (hits.length > 1 || !complete) return { kind: "several" };
  return { kind: "none" };
}

/**
 * The record that already stands for the found one (the vendor of a contact): an active one
 * first, else an archived one, which a person may restore instead of making a second.
 */
export function existing(
  rows: readonly Row[],
): { kind: "active"; id: string } | { kind: "archived"; id: string } | { kind: "none" } {
  const active = rows.find((r) => r.active !== false);
  if (active) return { kind: "active", id: active.id };
  const archived = rows[0];
  return archived ? { kind: "archived", id: archived.id } : { kind: "none" };
}
