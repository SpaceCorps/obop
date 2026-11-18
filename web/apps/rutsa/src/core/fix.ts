import type { NewFix } from "./types.ts";

/**
 * What the location task is given for one position: the shape of `LocationObject` of expo-location,
 * written out here so that `core/` imports nothing from Expo and the conversion runs in `vp test`.
 */
export interface Position {
  /** Milliseconds since the Unix epoch. */
  timestamp: number;
  coords: {
    latitude: number;
    longitude: number;
    /** Radius in metres. Android gives `null` or a positive number; CoreLocation gives a negative one for a position that is not valid. */
    accuracy: number | null;
    /** Metres per second. CoreLocation gives a negative one (-1) when it has no speed. */
    speed: number | null;
  };
}

/**
 * A position as the store takes it. The accuracy is passed on as it is: a negative one is how
 * CoreLocation says that the position is not valid, and `plausible` refuses it. A speed that is
 * negative or not a number is not a speed, so it becomes `null`.
 *
 * From `mileage-app` (`src/core/fix.ts`).
 */
export function toFix(position: Position): NewFix {
  const { coords } = position;
  const speed = coords.speed;
  return {
    t: position.timestamp,
    lat: coords.latitude,
    lon: coords.longitude,
    accuracyM: coords.accuracy,
    speedMps: speed !== null && Number.isFinite(speed) && speed >= 0 ? speed : null,
  };
}

/**
 * Whether a fix can be stored: a real time, a place on the Earth, and no accuracy that says the
 * position is not valid. A missing accuracy (`null`) is not a reason to refuse one. The tables
 * refuse what this refuses (`schema.ts`), so a fix that is not plausible is dropped before it can
 * fail a whole write.
 *
 * From `mileage-app` (`src/core/values.ts`).
 */
export function plausible(fix: NewFix): boolean {
  return (
    Number.isFinite(fix.t) &&
    Number.isFinite(fix.lat) &&
    Number.isFinite(fix.lon) &&
    Math.abs(fix.lat) <= 90 &&
    Math.abs(fix.lon) <= 180 &&
    !(fix.accuracyM !== null && !(fix.accuracyM >= 0))
  );
}
