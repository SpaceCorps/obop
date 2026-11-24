import { cleanText } from "../core/details.ts";
import type { Trip } from "../core/types.ts";

/**
 * What a trip is called on a line of the screen, from what the driver typed: its name, else
 * "From → To" (or the one of them that was typed), else `null`, and the screen shows its times
 * instead (the list of D5) or leaves the title out (the last trip of the home screen).
 */
export function tripTitle(trip: Pick<Trip, "name" | "from" | "to">): string | null {
  const name = cleanText(trip.name);
  if (name !== null) return name;
  const from = cleanText(trip.from);
  const to = cleanText(trip.to);
  if (from !== null && to !== null) return `${from} → ${to}`;
  return from ?? to;
}
