import type { TripDetails } from "./details.ts";
import type { TripProgress } from "./progress.ts";
import type { NewFix, StoredPoint, Trip } from "./types.ts";

/**
 * What the app can do with trips, without saying where they are kept. The screens and the
 * location task use only this; `sql-store.ts` keeps the trips in SQLite on the phone. A store that
 * also sends trips to a company's obop could be a second implementation of the same interface,
 * with no change to the screens (docs/spec/rutsa.md, the two products).
 *
 * Every time (`at`, `startedAt` ...) is milliseconds since the Unix epoch and is given by the
 * caller, never read from a clock here, so the rules can be tested with exact times.
 *
 * After `OutboxStore` of `mileage-app` (`src/core/store.ts`), without what talks to its server
 * (batches, submit, the server's states), and with pause, resume and trips added by hand.
 */
export interface TripRepository {
  /**
   * Starts a trip at `at`: state `recording`, segment 1, source `recorded`. Throws
   * `TripAlreadyActive` while another trip is recording or paused.
   */
  startTrip(at: number): Promise<Trip>;

  /** The trip that is recording or paused, or `null`. */
  activeTrip(): Promise<Trip | null>;

  /**
   * Stores the fixes of one delivery of the location service, in the trip's current segment, after
   * `acceptFixes` (`thin.ts`) has chosen them. Answers how many were stored. A trip that is paused,
   * ended or gone takes none and answers 0: a delivery that arrives after Pause or Stop is not an
   * error.
   */
  appendFixes(tripId: string, fixes: readonly NewFix[]): Promise<number>;

  /** `recording` to `paused`. Throws `TripStateError` from any other state. */
  pauseTrip(tripId: string, at: number): Promise<Trip>;

  /**
   * `paused` to `recording`, in a new segment that starts at `at` (not before the segment before
   * it, should the clock have been set back), so the distance does not count a straight line
   * across the pause, and a position from before `at` is not stored. Throws `TripStateError` from
   * any other state.
   */
  resumeTrip(tripId: string, at: number): Promise<Trip>;

  /**
   * `recording` or `paused` to `ended` at `at`; the distance is computed from the stored points,
   * segment by segment (`analyseTrack` of `@obop/trip-core`). The details typed at Stop (D4b), when
   * given, are stored in the same transaction, trimmed (`details.ts`); a detail that is too long
   * throws `DetailsTooLong` and nothing is changed. Throws `TripStateError` from `ended`.
   */
  stopTrip(tripId: string, at: number, details?: TripDetails): Promise<Trip>;

  /**
   * Restart: the open trip (`recording` or `paused`) starts again at `at`, as if Start had been
   * pressed now: its positions and segments are deleted, it records in a new segment 1 from `at`.
   * It stays the same trip: its id, its number (an untitled trip keeps "Untitled trip N", no new
   * number is taken) and its details. Throws `TripStateError` from `ended`, `TripNotFound` when
   * there is none.
   */
  restartTrip(tripId: string, at: number): Promise<Trip>;

  /**
   * Changes the details of a trip (D6), the open one too: its name, From, To and Note, all four at
   * once, trimmed (`details.ts`); a field left empty is cleared. A detail that is too long throws
   * `DetailsTooLong` and nothing is changed. Throws `TripNotFound` when there is none.
   */
  updateDetails(tripId: string, details: TripDetails): Promise<Trip>;

  /**
   * A trip the driver types afterwards (D7): ended, source `manual`, no points; its texts by the
   * rules of `details.ts` (`DetailsTooLong` refuses one that is too long). The checks of the typed
   * times and km against the clock and the other trips are made before (`core/manual.ts`).
   */
  addManualTrip(input: ManualTrip): Promise<Trip>;

  /** One trip, or `null` when there is none with this id. */
  getTrip(tripId: string): Promise<Trip | null>;

  /** Trips that started in `[from, to)` (either bound may be left out), newest first. */
  listTrips(range?: { from?: number; to?: number }): Promise<Trip[]>;

  /** The stored points of a trip in the order they were stored (`seq`). */
  points(tripId: string): Promise<StoredPoint[]>;

  /**
   * The count of a trip so far (`progress.ts`): its km by the same rule as Stop, its stored
   * positions, the newest one of its current segment and the note about 0 km. `null` when there
   * is no trip with this id.
   */
  progress(tripId: string): Promise<TripProgress | null>;

  /**
   * Deletes an ended trip with its points. Throws `TripStateError` for a trip that is recording or
   * paused (stop it first) and `TripNotFound` when there is none.
   */
  deleteTrip(tripId: string): Promise<void>;
}

/** What the driver types for a trip added by hand. */
export interface ManualTrip {
  startedAt: number;
  /** Not before `startedAt`. */
  endedAt: number;
  /** Metres, 0 or more. */
  distanceM: number;
  /** What the driver calls it; untitled, it gets the next number (D7). */
  name?: string | null;
  from?: string | null;
  to?: string | null;
  note?: string | null;
}

/** A trip is recording or paused already: there is at most one at a time. */
export class TripAlreadyActive extends Error {
  override name = "TripAlreadyActive";
  constructor(readonly activeTripId: string) {
    super("A trip is already being recorded");
  }
}

/** There is no trip with this id. */
export class TripNotFound extends Error {
  override name = "TripNotFound";
  constructor(readonly tripId: string) {
    super("There is no such trip");
  }
}

/** The action does not fit the trip's state (pausing an ended trip, deleting one that records ...). */
export class TripStateError extends Error {
  override name = "TripStateError";
  constructor(
    readonly tripId: string,
    readonly state: Trip["state"],
    readonly action: string,
  ) {
    super(`A trip that is ${state} cannot be ${action}`);
  }
}
