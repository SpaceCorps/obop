// The reminders of the Fleet home: how close a date is (a licence that expires, a planned service).
// Pure, so it is tested without a browser.
import type { ModelMeta, SearchQuery } from "../../api.ts";

/** The licences the home warns about: this many days ahead (the field's `warn_before_days`). */
export const LICENCE_WARN_DAYS = 30;

/** `day` plus `n` days, both `YYYY-MM-DD`. */
export function addDays(day: string, n: number): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
}

/** Whole days from `today` to `day` (negative once it is past). */
export function daysUntil(day: string, today: string): number {
  return Math.round(
    (Date.parse(`${day.slice(0, 10)}T00:00:00Z`) - Date.parse(`${today}T00:00:00Z`)) / 86_400_000,
  );
}

export type Urgency = "past" | "today" | "soon" | "later";

/** How a date reads against today: past, today, within `warn` days, or later. */
export function urgencyOf(day: string, today: string, warn: number): Urgency {
  const n = daysUntil(day, today);
  if (n < 0) return "past";
  if (n === 0) return "today";
  return n <= warn ? "soon" : "later";
}

/**
 * The search for the active drivers whose licence expires within `LICENCE_WARN_DAYS` of `day`,
 * soonest first; null when the reader does not see licence dates (the field is not in their
 * model), so the home asks nothing the server would refuse.
 */
export function licenceQuery(driver: Pick<ModelMeta, "fields"> | undefined, day: string) {
  if (!driver?.fields.some((f) => f.name === "licence_expiry")) return null;
  return {
    filter: [
      { field: "state", op: "eq", value: "active" },
      { field: "licence_expiry", op: "lte", value: addDays(day, LICENCE_WARN_DAYS) },
    ],
    order: [{ field: "licence_expiry", dir: "asc" }],
    limit: 50,
  } satisfies SearchQuery;
}
