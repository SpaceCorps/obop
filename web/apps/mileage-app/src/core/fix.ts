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
 * A position as the outbox takes it. The accuracy is passed on as it is: a negative one is how
 * CoreLocation says that the position is not valid, and `plausible` (`values.ts`) refuses it. A
 * speed that is negative or not a number is not a speed, so it becomes `null`.
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
