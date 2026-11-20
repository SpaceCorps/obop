import type { Readiness } from "./liveness.ts";
import type { TripRepository } from "./repository.ts";
import type { Trip } from "./types.ts";

/**
 * The location updates as the actions need them: whether the phone can record, and switching them
 * on and off. On the phone these are `locationReadiness` (`native/permissions.ts`) and
 * `startRecording` and `stopRecording` (`native/recorder.ts`); a test passes its own.
 */
export interface LocationUpdates {
  readiness(): Promise<Readiness>;
  /** Starts the updates that deliver positions to the background task. Throws when the system refuses. */
  start(): Promise<void>;
  /** Stops them; nothing happens when they are not running. */
  stop(): Promise<void>;
}

/** The phone cannot record a trip now; `reason` says what the driver has to change. */
export class CannotRecord extends Error {
  override name = "CannotRecord";
  constructor(readonly reason: Exclude<Readiness, "ready">) {
    super(`A trip cannot be recorded: ${reason}`);
  }
}

/** What the buttons Start, Pause, Resume and Stop do. */
export interface Recording {
  /**
   * Checks that the phone can record (`CannotRecord`), stores the trip (`TripAlreadyActive` while
   * another one is open), then starts the location updates. If they cannot start, the trip is
   * removed again and the error of the system is thrown: no trip is left that records nothing.
   */
  start(): Promise<Trip>;
  /**
   * Pauses the trip, then stops the location updates to save the battery. A delivery that comes in
   * between is not stored (`appendFixes` takes none for a paused trip). If the updates cannot be
   * stopped the trip stays paused: the fixes that keep coming are not stored.
   */
  pause(tripId: string): Promise<Trip>;
  /**
   * Checks that the phone can record (`CannotRecord`, the trip stays paused), resumes the trip in a
   * new segment, then starts the location updates. If they cannot start, the trip is paused again
   * and the error of the system is thrown.
   */
  resume(tripId: string): Promise<Trip>;
  /**
   * Stops the location updates, then ends the trip, which computes its distance. The trip is ended
   * also when the updates cannot be stopped (the system may have ended them already); the task
   * stops them at its next delivery, since no trip is active then.
   */
  stop(tripId: string): Promise<Trip>;
}

/**
 * The actions over a store and the location updates; `now` is the clock, so the tests give exact
 * times. Nothing here tells the screens: `native/actions.ts` does after each action.
 *
 * After `startTrip` and `stopTrip` of `mileage-app` (`src/native/actions.ts`), with Pause and
 * Resume, which it does not have; here the order of the steps and the rollbacks are tested.
 */
export function createRecording(deps: {
  store: TripRepository;
  location: LocationUpdates;
  now: () => number;
}): Recording {
  const { store, location, now } = deps;

  async function mustBeReady(): Promise<void> {
    const ready = await location.readiness();
    if (ready !== "ready") throw new CannotRecord(ready);
  }

  return {
    async start() {
      await mustBeReady();
      const trip = await store.startTrip(now());
      try {
        await location.start();
      } catch (error) {
        // `deleteTrip` takes only an ended trip, so the trip is ended first. If this fails too,
        // the error of the system is still the one the driver is told.
        await store
          .stopTrip(trip.id, now())
          .then(() => store.deleteTrip(trip.id))
          .catch(() => undefined);
        throw error;
      }
      return trip;
    },

    async pause(tripId) {
      const trip = await store.pauseTrip(tripId, now());
      await location.stop().catch(() => undefined);
      return trip;
    },

    async resume(tripId) {
      await mustBeReady();
      const trip = await store.resumeTrip(tripId, now());
      try {
        await location.start();
      } catch (error) {
        await store.pauseTrip(tripId, now()).catch(() => undefined);
        throw error;
      }
      return trip;
    },

    async stop(tripId) {
      await location.stop().catch(() => undefined);
      return store.stopTrip(tripId, now());
    },
  };
}
