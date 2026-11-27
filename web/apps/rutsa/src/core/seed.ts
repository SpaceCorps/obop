import type { TripRepository } from "./repository.ts";
import type { NewFix } from "./types.ts";

/**
 * TEMPORARY, removed before the stage is done (with `EXPO_PUBLIC_RUTSA_SEED`, F3): test trips for
 * an empty database, so the screens can be tried with weeks of trips on the emulator. A build made
 * with `EXPO_PUBLIC_RUTSA_SEED=1` writes them through the repository, as the app would, the first
 * time it opens an empty database (`native/store.ts`).
 *
 * The plan is pure and the same for the same `now` (a seeded random), so it is tested.
 */

/** One trip of the plan. Times in milliseconds since the epoch, distances in metres. */
export interface SeedTrip {
  source: "recorded" | "manual";
  startedAt: number;
  endedAt: number;
  /** Driven at this mean speed, in metres per second (recorded), or typed (manual: the metres). */
  speedMps: number;
  /** For a trip added by hand: its km as typed, in metres. */
  distanceM: number;
  /** A pause of a recorded trip: from, to (inside the trip). */
  pause: { at: number; until: number } | null;
  name: string | null;
  from: string | null;
  to: string | null;
  note: string | null;
}

/** How many days back the plan goes, today included. */
export const SEED_DAYS = 45;

const MIN = 60_000;
const HOUR = 60 * MIN;

/** The places the trips go between (Stockholm and around). */
const PLACES = [
  "Office",
  "Warehouse Västberga",
  "Client in Solna",
  "Arlanda",
  "Kista",
  "Uppsala depot",
  "Södertälje site",
  "Home",
];
const NAMES = ["Client meeting", "Delivery run", "Site visit", "Airport pick-up", "Supplier"];
const NOTES = [
  "Parking 40 kr, receipt in the glovebox.",
  "Picked up samples on the way back.",
  "Road works on E4, detour through Kungens kurva.",
];

/** A small seeded random in [0, 1) (mulberry32), so the plan repeats for the same seed. */
function seededRandom(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}

/**
 * The trips of the last `SEED_DAYS` days up to `now`, oldest first: one to three on a weekday, now
 * and then one on a weekend, between 07:00 and 19:00 (today only those that ended 10 minutes before
 * `now`); about a third added by hand, some recorded ones with a pause, some with a name, places
 * and a note. `midnightOf` gives the start of the phone's day of a time.
 */
export function seedPlan(now: number, midnightOf: (ms: number) => number): SeedTrip[] {
  const random = seededRandom(Math.floor(now / HOUR));
  const pick = <T>(list: readonly T[]): T => list[Math.floor(random() * list.length)]!;
  const between = (low: number, high: number) => low + random() * (high - low);
  const trips: SeedTrip[] = [];
  const today = midnightOf(now);

  for (let back = SEED_DAYS - 1; back >= 0; back -= 1) {
    // A day back from today's midnight, by the phone's day (a change of the clocks moves it).
    const midnight = midnightOf(today - back * 24 * HOUR + 12 * HOUR);
    const weekday = new Date(midnight + 12 * HOUR).getDay();
    const weekend = weekday === 0 || weekday === 6;
    const count = weekend ? (random() < 0.3 ? 1 : 0) : 1 + Math.floor(random() * 3);
    const dayEnd = back === 0 ? now - 10 * MIN : midnight + 19 * HOUR;
    let at = midnight + between(7, 9) * HOUR;

    for (let n = 0; n < count; n += 1) {
      const minutes = Math.round(between(10, 75));
      const startedAt = Math.round(at / MIN) * MIN;
      const endedAt = startedAt + minutes * MIN;
      if (endedAt > dayEnd) break;
      const speedMps = between(8, 22);
      const manual = random() < 0.3;
      const pause =
        !manual && minutes >= 30 && random() < 0.3
          ? {
              at: startedAt + Math.round(minutes * 0.4) * MIN,
              until: startedAt + Math.round(minutes * 0.4 + 8) * MIN,
            }
          : null;
      const placed = random() < 0.5;
      const from = placed ? pick(PLACES) : null;
      const to = placed ? pick(PLACES.filter((place) => place !== from)) : null;
      trips.push({
        source: manual ? "manual" : "recorded",
        startedAt,
        endedAt,
        speedMps,
        distanceM: Math.round((speedMps * minutes * 60) / 100) * 100,
        pause,
        name: random() < 0.25 ? pick(NAMES) : null,
        from,
        to,
        note: random() < 0.15 ? pick(NOTES) : null,
      });
      at = endedAt + between(1, 3) * HOUR;
    }
  }
  return trips;
}

/** A fix every this many milliseconds along a recorded trip. */
const FIX_EVERY_MS = 15_000;

/** About this many metres of latitude in a degree. */
const METRES_PER_DEGREE = 111_195;

/**
 * The fixes of `[from, until)` of a recorded trip: a straight line from `start` (degrees) at
 * `speedMps`, heading `bearing` (radians), `doneM` metres along it at `from`.
 */
function fixesOf(
  from: number,
  until: number,
  speedMps: number,
  start: { lat: number; lon: number },
  bearing: number,
  doneM: number,
): NewFix[] {
  const fixes: NewFix[] = [];
  for (let t = from + FIX_EVERY_MS / 3; t < until; t += FIX_EVERY_MS) {
    const metres = doneM + (speedMps * (t - from)) / 1000;
    const lat = start.lat + (metres * Math.cos(bearing)) / METRES_PER_DEGREE;
    const lon =
      start.lon +
      (metres * Math.sin(bearing)) / (METRES_PER_DEGREE * Math.cos((start.lat * Math.PI) / 180));
    fixes.push({ t: Math.round(t), lat, lon, accuracyM: 5, speedMps });
  }
  return fixes;
}

/**
 * Writes the plan into `store`, as the app would: a recorded trip is started, gets its fixes
 * (paused and resumed in the middle when the plan says so) and is stopped with its details; a trip
 * added by hand is added. Answers how many trips were written.
 */
export async function applySeed(store: TripRepository, plan: readonly SeedTrip[]): Promise<number> {
  const random = seededRandom(plan.length);
  for (const trip of plan) {
    const details = { name: trip.name, from: trip.from, to: trip.to, note: trip.note };
    if (trip.source === "manual") {
      await store.addManualTrip({
        startedAt: trip.startedAt,
        endedAt: trip.endedAt,
        distanceM: trip.distanceM,
        ...details,
      });
      continue;
    }
    const start = { lat: 59.33 + (random() - 0.5) * 0.2, lon: 18.06 + (random() - 0.5) * 0.4 };
    const bearing = random() * 2 * Math.PI;
    const recorded = await store.startTrip(trip.startedAt);
    if (trip.pause) {
      const before = fixesOf(trip.startedAt, trip.pause.at, trip.speedMps, start, bearing, 0);
      await store.appendFixes(recorded.id, before);
      await store.pauseTrip(recorded.id, trip.pause.at);
      await store.resumeTrip(recorded.id, trip.pause.until);
      const doneM = (trip.speedMps * (trip.pause.at - trip.startedAt)) / 1000;
      const after = fixesOf(trip.pause.until, trip.endedAt, trip.speedMps, start, bearing, doneM);
      await store.appendFixes(recorded.id, after);
    } else {
      const fixes = fixesOf(trip.startedAt, trip.endedAt, trip.speedMps, start, bearing, 0);
      await store.appendFixes(recorded.id, fixes);
    }
    await store.stopTrip(recorded.id, trip.endedAt, details);
  }
  return plan.length;
}
