// The trip review of Mileage: the route of a trip drawn from its points, and what the server's
// check of the track found, in words. Pure, so it is tested without a browser.
import { fmt, t } from "@obop/ui";
import { analyseTrack, type Fix } from "@obop/trip-core";
import type { Row } from "../../api.ts";

/** The server drops a fix whose accuracy radius is above this (mileage `track.rs` MAX_ACCURACY_M). */
export const MAX_ACCURACY_M = 100;

export type Point = {
  lat: number;
  lon: number;
  accuracy: number | null;
  /** The sequence number and the time of the fix (ms), when the row has them. */
  seq?: number;
  t?: number;
};

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
    const seq = num(r.seq);
    const t = typeof r.recorded_at === "string" ? Date.parse(r.recorded_at) : Number.NaN;
    out.push({
      lat,
      lon,
      accuracy: num(r.accuracy_m),
      ...(seq !== null && Number.isFinite(t) ? { seq, t } : {}),
    });
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
 * The fixes the server's check of the track keeps, in stretches (`analyseTrack` of
 * `@obop/trip-core`, the rules the server computes the distance with): a fix of poor accuracy, one
 * from before the start, one that jumped further than a car can drive, and the fixes before a
 * re-anchor of a trip that had not moved yet (a far-off first fix) are left out (UB10). Points
 * without a sequence number and a time are judged by their accuracy alone, as one stretch.
 */
function stretchesOf(points: readonly Point[], startedAt?: string | null): Point[][] {
  if (points.every((p) => p.seq !== undefined && p.t !== undefined)) {
    const fixes: Fix[] = points.map((p) => ({
      seq: p.seq!,
      t: p.t!,
      lat: p.lat,
      lon: p.lon,
      accuracyM: p.accuracy,
    }));
    const start = startedAt ? Date.parse(startedAt) : Number.NaN;
    const a = analyseTrack(fixes, Number.isFinite(start) ? { startedAt: start } : {});
    const bySeq = new Map(points.map((p) => [p.seq!, p]));
    return a.stretches.map((s) => s.map((seq) => bySeq.get(seq)!));
  }
  const kept = points.filter(accepted);
  return kept.length ? [kept] : [];
}

/**
 * Fit the route into a box keeping its shape (a local flat projection: a degree of longitude is
 * shorter by the cosine of the latitude), with `pad` around it. The route is the fixes that the
 * check of the track keeps (`stretchesOf`); the others are the dropped points.
 */
export function drawRoute(
  points: readonly Point[],
  width: number,
  height: number,
  pad = 16,
  startedAt?: string | null,
): Drawn {
  const empty: Drawn = { path: "", start: null, end: null, dropped: [] };
  if (!points.length) return empty;
  const stretches = stretchesOf(points, startedAt);
  const keep = new Set(stretches.flat());
  // The box fits the kept route; one far-off dropped fix must not shrink it to a dot.
  const fit = keep.size ? [...keep] : points;
  const lat0 = (fit.reduce((s, p) => s + p.lat, 0) / fit.length) * (Math.PI / 180);
  const k = Math.cos(lat0);
  const proj = (p: Point) => [p.lon * k, -p.lat] as const;
  const xy = fit.map(proj);
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
  const round = (v: number) => Math.round(v * 10) / 10;
  // A dropped point outside the box is pinned to its edge.
  const pin = (v: number, max: number) => Math.min(Math.max(v, 2), max - 2);
  const at = (p: Point): [number, number] => {
    const [x, y] = proj(p);
    return [
      round(pin(offX + (x - minX) * scale, width)),
      round(pin(offY + (y - minY) * scale, height)),
    ];
  };
  const dropped = points.filter((p) => !keep.has(p)).map(at);
  // Each stretch is its own line: the jump between two is not part of the route.
  const lines = stretches.map((s) => s.map(at));
  const path = lines
    .flatMap((line) => line.map(([x, y], i) => `${i ? "L" : "M"}${x} ${y}`))
    .join(" ");
  const first = lines[0]?.[0] ?? null;
  const lastLine = lines[lines.length - 1];
  const end = lastLine ? lastLine[lastLine.length - 1]! : null;
  return { path, start: first, end, dropped };
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

/**
 * A plain sentence for the server's refusal of Confirm or Reject on a trip that is no longer in
 * review (someone reviewed it meanwhile); null for any other refusal.
 */
export function reviewRefusal(message: string): string | null {
  return message.startsWith("state must be in_review to do this, it is ")
    ? t("This trip was already reviewed.")
    : null;
}

export type Note = { tone: "ok" | "warn"; text: string };

/**
 * What the server's check of the track found (`integrity`), as sentences. "Clean" is said only
 * when the server set no flag at all, so a flag this screen has no sentence for never reads as a
 * clean track.
 */
export function checkNotes(integrity: unknown, total: number, ended: boolean): Note[] {
  const flags = flagsOf(integrity);
  const notes: Note[] = [];
  const n = (k: string) => fmt.integer(flags[k] ?? 0);
  if (flags.low_accuracy)
    notes.push({
      tone: "warn",
      text: t("{n} of {total} points had poor GPS accuracy and were left out of the distance.", {
        n: n("low_accuracy"),
        total: fmt.integer(total),
      }),
    });
  if (flags.speed_jump)
    notes.push({
      tone: "warn",
      text: t("{n} points jumped further than a car can drive and were left out.", {
        n: n("speed_jump"),
      }),
    });
  if (flags.seq_gap)
    notes.push({
      tone: "warn",
      text: t("{n} gaps in the recording: the phone sent fewer points than it counted.", {
        n: n("seq_gap"),
      }),
    });
  if (flags.before_start)
    notes.push({
      tone: "warn",
      text:
        flags.before_start === 1
          ? t("1 point was recorded before the trip started and was left out.")
          : t("{n} points were recorded before the trip started and were left out.", {
              n: n("before_start"),
            }),
    });
  if (flags.route_removed)
    notes.push({
      tone: "warn",
      text: t(
        "The route was removed when the trip was marked private ({n} points): the distance cannot be checked against a route.",
        { n: n("route_removed") },
      ),
    });
  if (flags.reanchor)
    notes.push({
      tone: "warn",
      text:
        flags.reanchor === 1
          ? t("The track jumped to a new place and went on from there; the jump was not counted.")
          : t(
              "The track jumped to a new place {n} times and went on from there; the jumps were not counted.",
              { n: n("reanchor") },
            ),
    });
  const flagged = Object.keys(flags).length > 0;
  if (!notes.length && flagged)
    notes.push({ tone: "warn", text: t("The check of the track found something to look at.") });
  // A track of no points is not clean: there is nothing to judge.
  if (!flagged && ended && total > 0)
    notes.push({
      tone: "ok",
      text: t("The track is clean: all {total} points were used for the distance.", {
        total: fmt.integer(total),
      }),
    });
  return notes;
}
