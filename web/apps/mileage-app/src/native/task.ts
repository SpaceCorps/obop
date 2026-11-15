import * as Location from "expo-location";
import * as TaskManager from "expo-task-manager";
import type { NewFix } from "../core/types.ts";
import { emitChange } from "./bus.ts";
import { LOCATION_TASK } from "./config.ts";
import { getStore } from "./store.ts";

/**
 * The background location task. It must be defined at the top level of a module that the entry
 * point imports before the app starts (expo-task-manager runs it when the service delivers a
 * fix, also when no screen exists). It does one thing: write the fixes to the encrypted outbox
 * before anything else can go wrong. Uploading is the app's job, in `runtime.ts`.
 *
 * Nothing here logs a coordinate or a token.
 */
interface LocationTaskData {
  locations?: Location.LocationObject[];
}

export function toFix(location: Location.LocationObject): NewFix {
  const { coords } = location;
  return {
    t: location.timestamp,
    lat: coords.latitude,
    lon: coords.longitude,
    accuracyM: coords.accuracy,
    speedMps: coords.speed,
  };
}

TaskManager.defineTask<LocationTaskData>(LOCATION_TASK, async ({ data, error }) => {
  if (error) {
    console.warn(`location task: ${error.message}`);
    return;
  }
  const locations = data?.locations ?? [];
  if (locations.length === 0) return;
  try {
    const store = await getStore();
    const active = await store.activeTrip();
    if (!active) {
      // No trip is recording (it was stopped or discarded): the service has no reason to run.
      await Location.stopLocationUpdatesAsync(LOCATION_TASK).catch(() => undefined);
      return;
    }
    const stored = await store.appendFixes(active.clientTripId, locations.map(toFix));
    if (stored > 0) emitChange();
  } catch (e) {
    console.warn(
      `location task could not store a fix: ${e instanceof Error ? e.message : "error"}`,
    );
  }
});
