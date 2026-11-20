import { createRecording, type Recording } from "../core/recording.ts";
import type { Trip } from "../core/types.ts";
import { emitChange } from "./bus.ts";
import { locationReadiness } from "./permissions.ts";
import { startRecording, stopRecording } from "./recorder.ts";
import { getStore } from "./store.ts";

export { CannotRecord } from "../core/recording.ts";

/**
 * What the buttons Start, Pause, Resume and Stop call: `core/recording.ts` over the store of the
 * phone, the location updates and the clock. After each action the open screens are told, also
 * when it failed, since a rollback changes the trips too.
 *
 * After `mileage-app` (`src/native/actions.ts`), without what talks to its server.
 */
async function run(action: (recording: Recording) => Promise<Trip>): Promise<Trip> {
  try {
    const recording = createRecording({
      store: await getStore(),
      location: { readiness: locationReadiness, start: startRecording, stop: stopRecording },
      now: Date.now,
    });
    return await action(recording);
  } finally {
    emitChange();
  }
}

export function startTrip(): Promise<Trip> {
  return run((r) => r.start());
}

export function pauseTrip(tripId: string): Promise<Trip> {
  return run((r) => r.pause(tripId));
}

export function resumeTrip(tripId: string): Promise<Trip> {
  return run((r) => r.resume(tripId));
}

export function stopTrip(tripId: string): Promise<Trip> {
  return run((r) => r.stop(tripId));
}
