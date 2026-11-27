/**
 * The details of a trip the driver types (at Stop, D4b; later on the trip's screen): its name, where
 * it began and ended, and a note. All optional. Pure, so the rules are tested once and used by the
 * window of Stop (each field's message) and by the store (which refuses what breaks them).
 */
export interface TripDetails {
  name?: string | null;
  from?: string | null;
  to?: string | null;
  note?: string | null;
}

/** The details as they are stored: trimmed, `null` for nothing. */
export interface StoredDetails {
  name: string | null;
  from: string | null;
  to: string | null;
  note: string | null;
}

/** The longest text each field takes, in characters (Unicode code points). */
export const DETAIL_MAX: Readonly<Record<keyof StoredDetails, number>> = {
  name: 100,
  from: 200,
  to: 200,
  note: 1000,
};

/** A typed text: trimmed, and `null` when nothing is left. */
export function cleanText(value: string | null | undefined): string | null {
  const trimmed = value?.trim() ?? "";
  return trimmed === "" ? null : trimmed;
}

/** The length of a text in Unicode code points (an emoji of one code point counts once). */
export function codePoints(value: string): number {
  let count = 0;
  for (const _ of value) count += 1;
  return count;
}

/** The fields that are too long, after trimming; empty when all are fine. */
export function detailsProblems(details: TripDetails): (keyof StoredDetails)[] {
  const problems: (keyof StoredDetails)[] = [];
  for (const field of ["name", "from", "to", "note"] as const) {
    const value = cleanText(details[field]);
    if (value !== null && codePoints(value) > DETAIL_MAX[field]) problems.push(field);
  }
  return problems;
}

/** A detail is too long for its field. */
export class DetailsTooLong extends Error {
  override name = "DetailsTooLong";
  constructor(readonly fields: (keyof StoredDetails)[]) {
    super(`Too long: ${fields.join(", ")}`);
  }
}

/** The details as they are stored; throws `DetailsTooLong` when a field is over its limit. */
export function cleanDetails(details: TripDetails): StoredDetails {
  const problems = detailsProblems(details);
  if (problems.length > 0) throw new DetailsTooLong(problems);
  return {
    name: cleanText(details.name),
    from: cleanText(details.from),
    to: cleanText(details.to),
    note: cleanText(details.note),
  };
}
