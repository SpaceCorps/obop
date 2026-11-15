import type { NewFix, StoredPoint } from "./types.ts";

/**
 * A random UUID of version 4 from 16 random bytes (RFC 9562 section 5.4). Version 4 and not 7 on
 * purpose: the first 48 bits of a version 7 are the time it was made, and an agent's token reads
 * `client_trip_id` in the clear while `started_at` is withheld from it (docs/spec/mobile.md 5.3).
 */
export function uuidV4(random: Uint8Array): string {
  if (random.length < 16) throw new RangeError("a UUID needs 16 random bytes");
  const b = Array.from(random.slice(0, 16));
  b[6] = (b[6]! & 0x0f) | 0x40;
  b[8] = (b[8]! & 0x3f) | 0x80;
  const hex = b.map((x) => x.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/** The instant of a time in milliseconds, as RFC 3339 in UTC with milliseconds (`2026-10-15T09:00:00.123Z`). */
export function instant(ms: number): string {
  return new Date(ms).toISOString();
}

/** A decimal text with `scale` digits after the point, without a minus sign on a zero. */
export function fixed(value: number, scale: number): string {
  const text = value.toFixed(scale);
  return Number(text) === 0 ? (0).toFixed(scale) : text;
}

/**
 * A fix as the texts that are stored and sent: latitude and longitude with 7 decimals (the field
 * says at most 7), accuracy and speed with 2. A value the platform did not give, or gave as a
 * negative number or a number the field cannot hold, is left out, because both are optional
 * (`accuracy_m` decimal(8,2), `speed_mps` decimal(7,2)).
 */
export function toStored(seq: number, fix: NewFix): StoredPoint {
  const optional = (value: number | null, scale: number, max: number): string | null =>
    value !== null && Number.isFinite(value) && value >= 0 && value < max
      ? fixed(value, scale)
      : null;
  return {
    seq,
    t: fix.t,
    lat: fixed(fix.lat, 7),
    lon: fixed(fix.lon, 7),
    accuracyM: optional(fix.accuracyM, 2, 1e6),
    speedMps: optional(fix.speedMps, 2, 1e5),
  };
}

/**
 * A fix may be older than the moment of Start by this much and still belong to the trip (a fix
 * made just before the tap). An older one is the position the phone had cached from before the
 * trip; the first run of the app on an emulator stored such a fix as point 1 (900 m and six
 * minutes away) and the server counted the leg to the first real fix.
 */
export const STALE_FIX_MS = 10_000;

/** Whether a fix is a position the phone can trust to be on Earth. */
export function plausible(fix: NewFix): boolean {
  return (
    Number.isFinite(fix.t) &&
    Number.isFinite(fix.lat) &&
    Number.isFinite(fix.lon) &&
    Math.abs(fix.lat) <= 90 &&
    Math.abs(fix.lon) <= 180
  );
}

/** One row of `mileage_point.bulk_create`. */
export interface PointRow {
  trip: string;
  seq: number;
  recorded_at: string;
  lat: string;
  lon: string;
  accuracy_m?: string;
  speed_mps?: string;
}

export function pointRow(serverTripId: string, p: StoredPoint): PointRow {
  const row: PointRow = {
    trip: serverTripId,
    seq: p.seq,
    recorded_at: instant(p.t),
    lat: p.lat,
    lon: p.lon,
  };
  if (p.accuracyM !== null) row.accuracy_m = p.accuracyM;
  if (p.speedMps !== null) row.speed_mps = p.speedMps;
  return row;
}

/** The idempotency key of the creation of a trip (docs/spec/mobile.md 5.3). */
export const tripKey = (clientTripId: string): string => `trip:${clientTripId}`;

/** The idempotency key of a batch: it names the range, which never changes once it is queued. */
export const pointsKey = (clientTripId: string, firstSeq: number, lastSeq: number): string =>
  `points:${clientTripId}:${firstSeq}-${lastSeq}`;

/** "1 h 05 min" style text of a time span in milliseconds. */
export function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const two = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${two(m)}:${two(s)}`;
}
