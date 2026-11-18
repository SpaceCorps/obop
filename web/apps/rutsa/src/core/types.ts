/**
 * What the phone keeps about a trip. Pure data: nothing in `core/` imports React Native, Expo or a
 * Node API, so the logic runs in `vp test` and on the phone alike.
 *
 * Times are milliseconds since the Unix epoch, which is UTC; days and clock times are shown in the
 * phone's time zone (`present.ts`). Distances are metres.
 */

/** `recording`: positions are taken. `paused`: none are taken until Resume. `ended`: Stop was pressed. */
export type TripState = "recording" | "paused" | "ended";
export const TRIP_STATES: readonly TripState[] = ["recording", "paused", "ended"];

/** `recorded`: from the phone's positions. `manual`: typed by the driver, with no positions. */
export type TripSource = "recorded" | "manual";
export const TRIP_SOURCES: readonly TripSource[] = ["recorded", "manual"];

export interface Trip {
  id: string;
  state: TripState;
  source: TripSource;
  startedAt: number;
  /** `null` until the trip has ended. */
  endedAt: number | null;
  /**
   * Recorded: computed at Stop from the positions (`analyseTrack` of `@obop/trip-core`), `null`
   * before. Manual: as typed.
   */
  distanceM: number | null;
  /** The segment the next positions belong to: 1 at Start, one more after each Resume. */
  segment: number;
  /** Where the trip began and ended, as the driver wrote it (manual trips); `null` when not given. */
  from: string | null;
  to: string | null;
  note: string | null;
}

/**
 * A position as the location service gives it, before it is stored. The shape the location task
 * hands over (`fix.ts` converts it).
 */
export interface NewFix {
  t: number;
  lat: number;
  lon: number;
  /** Radius in metres; `null` when the platform did not say. */
  accuracyM: number | null;
  /** Metres per second; `null` when the platform did not say (or said a negative number). */
  speedMps: number | null;
}

/** A stored position of a recorded trip. */
export interface StoredPoint extends NewFix {
  tripId: string;
  /** 1, 2, 3 ... in the order the positions were stored, over the whole trip. */
  seq: number;
  /** The trip's segment when the position was stored; a pause starts a new one. */
  segment: number;
}
