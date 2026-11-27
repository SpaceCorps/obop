import { cleanText } from "../core/details.ts";
import type { Trip } from "../core/types.ts";

/**
 * What a trip is called on a line of the screen, from what the driver typed: its name, else
 * "From → To" (or the one of them that was typed), else `null`, and the screen shows its times
 * instead (the list of D5).
 */
export function tripTitle(trip: Pick<Trip, "name" | "from" | "to">): string | null {
  const name = cleanText(trip.name);
  if (name !== null) return name;
  const from = cleanText(trip.from);
  const to = cleanText(trip.to);
  if (from !== null && to !== null) return `${from} → ${to}`;
  return from ?? to;
}

/**
 * The name of a trip on its screen (D6): what the driver called it, else the default of its number
 * (`untitled(12)` is "Untitled trip 12" in the catalogue; `untitled(null)` for a trip without a
 * number, which the store does not make). The default is not stored: a name that is cleared and
 * saved shows a default again.
 */
export function tripName(
  trip: Pick<Trip, "name" | "number">,
  untitled: (number: number | null) => string,
): string {
  return cleanText(trip.name) ?? untitled(trip.number);
}

/**
 * The name to store from the field of the window of a trip's details, which opens with the default
 * name of an untitled trip filled in: left as it is (trimmed), it is no name, so the trip stays
 * untitled and keeps its number.
 */
export function typedName(
  typed: string | null | undefined,
  defaultName: string | null,
): string | null {
  const name = cleanText(typed);
  return name !== null && name === defaultName ? null : name;
}
