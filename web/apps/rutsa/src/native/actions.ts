import { createRecording, type Ensured, type Recording } from "../core/recording.ts";
import type { Trip } from "../core/types.ts";
import { emitChange } from "./bus.ts";
import { locationReadiness } from "./permissions.ts";
import { startRecording, stopRecording } from "./recorder.ts";
import { getStore } from "./store.ts";

export { CannotRecord } from "../core/recording.ts";

/**
 * What the buttons Start, Pause, Resume and Stop call, and the check of a recording when the app
 * opens: `core/recording.ts` over the store of the
 * phone, the location updates and the clock. After each action the open screens are told, also
 * when it failed, since a rollback changes the trips too.
 *
 * After `mileage-app` (`src/native/actions.ts`), without what talks to its server.
 */
async function run<T>(action: (recording: Recording) => Promise<T>): Promise<T> {
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

/**
 * Starts the recording of the open trip again after the app's process was ended, or stops updates
 * that run without a recording trip (`Recording.ensure`). Called when the app opens and whenever
 * it comes back to the front.
 */
export function ensureRecording(): Promise<Ensured> {
  return run((r) => r.ensure());
}
