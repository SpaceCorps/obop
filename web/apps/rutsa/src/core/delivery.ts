import { toFix, type Position } from "./fix.ts";
import type { TripRepository } from "./repository.ts";

/** What became of one delivery of the location service. */
export interface Delivery {
  /** How many fixes were stored (after `acceptFixes` chose them). */
  stored: number;
  /**
   * No trip is recording or paused (it was stopped, or deleted after a stop): the location updates
   * have no reason to run, and the task stops them to save the battery.
   */
  stopUpdates: boolean;
}

/**
 * What the background location task does with one delivery: the positions go to the active trip,
 * converted by `toFix` and chosen by `appendFixes`. A paused trip takes none and keeps the updates
 * running (Pause stops them itself, C4 of the plan); with no active trip the updates are stopped. An
 * empty delivery changes nothing.
 *
 * In `mileage-app` this is the body of the task (`src/native/task.ts`); here it is pure, so it runs
 * in `vp test` against a real store, and `native/task.ts` only hands it the positions.
 */
export async function storeDelivery(
  store: Pick<TripRepository, "activeTrip" | "appendFixes">,
  positions: readonly Position[],
): Promise<Delivery> {
  if (positions.length === 0) return { stored: 0, stopUpdates: false };
  const active = await store.activeTrip();
  if (active === null) return { stored: 0, stopUpdates: true };
  const stored = await store.appendFixes(active.id, positions.map(toFix));
  return { stored, stopUpdates: false };
}
