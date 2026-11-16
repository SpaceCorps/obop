// The trip review of Mileage: the route of a trip drawn from its points, and what the server's
// check of the track found, in words. Pure, so it is tested without a browser.
import type { Row } from "../../api.ts";

/** The server drops a fix whose accuracy radius is above this (mileage `track.rs` MAX_ACCURACY_M). */
export const MAX_ACCURACY_M = 100;

export type Point = { lat: number; lon: number; accuracy: number | null };

const num = (v: unknown): number | null => {
  if (v === null || v === undefined || v === "") return null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
};

/** The points of a trip with a usable position, in the order they came. */
export function pointsOf(rows: readonly Row[]): Point[] {
  const out: Point[] = [];
  for (const r of rows) {
    const lat = num(r.lat);
    const lon = num(r.lon);
    if (lat === null || lon === null || Math.abs(lat) > 90 || Math.abs(lon) > 180) continue;
    out.push({ lat, lon, accuracy: num(r.accuracy_m) });
  }
  return out;
}

/** A point the server would use for the distance: its accuracy is known to be good enough. */
export const accepted = (p: Point) => p.accuracy === null || p.accuracy <= MAX_ACCURACY_M;

export type Drawn = {
  /** The SVG path of the accepted points, in a `width` x `height` box. */
  path: string;
  start: [number, number] | null;
  end: [number, number] | null;
  /** The dropped points, to show faintly. */
  dropped: [number, number][];
};

/**
 * Fit the route into a box keeping its shape (a local flat projection: a degree of longitude is
 * shorter by the cosine of the latitude), with `pad` around it.
 */
export function drawRoute(
  points: readonly Point[],
  width: number,
  height: number,
  pad = 16,
): Drawn {
  const empty: Drawn = { path: "", start: null, end: null, dropped: [] };
  if (!points.length) return empty;
  const lat0 = (points.reduce((s, p) => s + p.lat, 0) / points.length) * (Math.PI / 180);
  const k = Math.cos(lat0);
  const xy = points.map((p) => [p.lon * k, -p.lat] as const);
  const xs = xy.map((p) => p[0]);
  const ys = xy.map((p) => p[1]);
  const [minX, maxX, minY, maxY] = [
    Math.min(...xs),
    Math.max(...xs),
    Math.min(...ys),
    Math.max(...ys),
  ];
  const spanX = maxX - minX || 1e-9;
  const spanY = maxY - minY || 1e-9;
  const scale = Math.min((width - 2 * pad) / spanX, (height - 2 * pad) / spanY);
  const offX = (width - spanX * scale) / 2;
  const offY = (height - spanY * scale) / 2;
  const at = (i: number): [number, number] => [
    Math.round((offX + (xy[i]![0] - minX) * scale) * 10) / 10,
    Math.round((offY + (xy[i]![1] - minY) * scale) * 10) / 10,
  ];
  const kept: [number, number][] = [];
  const dropped: [number, number][] = [];
  points.forEach((p, i) => (accepted(p) ? kept : dropped).push(at(i)));
  const path = kept.map(([x, y], i) => `${i ? "L" : "M"}${x} ${y}`).join(" ");
  return { path, start: kept[0] ?? null, end: kept[kept.length - 1] ?? null, dropped };
}

/** The flags the server set when the trip ended (`seq_gap:2 speed_jump:1 low_accuracy:14`). */
export function flagsOf(integrity: unknown): Record<string, number> {
  const out: Record<string, number> = {};
  if (typeof integrity !== "string") return out;
  for (const part of integrity.split(/[\s,;]+/)) {
    const [k, v] = part.split(":");
    const n = Number(v);
    if (k && Number.isFinite(n) && n > 0) out[k] = n;
  }
  return out;
}

/** Kilometres from the stored metres (a decimal string), one decimal; null when there are none. */
export function kmOf(distanceM: unknown): number | null {
  const m = num(distanceM);
  return m === null ? null : Math.round(m / 100) / 10;
}

/** Minutes between two instants, or null. */
export function minutesBetween(from: unknown, to: unknown): number | null {
  if (typeof from !== "string" || typeof to !== "string") return null;
  const ms = Date.parse(to) - Date.parse(from);
  return Number.isFinite(ms) && ms >= 0 ? Math.round(ms / 60_000) : null;
}
