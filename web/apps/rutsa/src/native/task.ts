import * as Location from "expo-location";
import * as TaskManager from "expo-task-manager";
import { storeDelivery } from "../core/delivery.ts";
import { emitChange } from "./bus.ts";
import { LOCATION_TASK } from "./config.ts";
import { getStore } from "./store.ts";

/**
 * The background location task. It must be defined at the top level of a module that the entry
 * point imports before the app is registered (`index.ts`): expo-task-manager runs it when the
 * location service delivers positions, also when no screen exists. What it does with them is
 * `storeDelivery` (`core/delivery.ts`), where it is tested; here it only stops the updates when no
 * trip is active and tells the open screens that fixes were stored.
 *
 * Nothing here logs a coordinate.
 *
 * After `mileage-app` (`src/native/task.ts`).
 */
interface LocationTaskData {
  locations?: Location.LocationObject[];
}

TaskManager.defineTask<LocationTaskData>(LOCATION_TASK, async ({ data, error }) => {
  if (error) {
    console.warn(`location task: ${error.message}`);
    return;
  }
  const locations = data?.locations ?? [];
  if (locations.length === 0) return;
  try {
    const { stored, stopUpdates } = await storeDelivery(await getStore(), locations);
    if (stopUpdates) {
      await Location.stopLocationUpdatesAsync(LOCATION_TASK).catch(() => undefined);
    }
    if (stored > 0) emitChange();
  } catch (e) {
    console.warn(
      `location task could not store a fix: ${e instanceof Error ? e.message : "error"}`,
    );
  }
});
