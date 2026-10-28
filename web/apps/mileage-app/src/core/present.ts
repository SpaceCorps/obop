import { formatNumber, t, tp, type MessageKey, type PluralKey } from "../i18n/index.ts";
import { hasWork } from "./store.ts";
import type { LocalTrip, ServerState } from "./types.ts";

/** What a trip is, in the words the driver sees (the keys of `state.*` in the catalogue). */
export type TripStatus = "recording" | "uploading" | "waiting" | "problem" | ServerState;

/**
 * The status of a trip on this phone. A refusal for good outranks everything; a trip that is still
 * open is recording; a trip the server does not hold in full is waiting (not on the server yet) or
 * uploading (on the server, points or steps still to send); otherwise the server's own state.
 */
export function tripStatus(trip: LocalTrip): TripStatus {
  if (trip.problem !== null && trip.problem.code !== "incomplete") return "problem";
  if (trip.origin === "device" && !trip.stopRequested && trip.discardReason === null) {
    return "recording";
  }
  if (trip.discardDone) return "discarded";
  if (hasWork(trip)) return trip.serverId === null ? "waiting" : "uploading";
  return trip.serverState ?? (trip.endDone ? "ended" : "waiting");
}

export const statusText = (status: TripStatus): string => t(`state.${status}` as MessageKey);

/** A distance in metres given as a decimal text (`"12345.67"`) as kilometres or metres for a driver. */
export function formatDistance(metres: string | null): string | null {
  if (metres === null) return null;
  const m = Number(metres);
  if (!Number.isFinite(m)) return null;
  // The number and its unit come from the language, not from this code (decimal separator, unit text).
  return m >= 1000
    ? t("unit.km", { value: formatNumber(m / 1000, 1) })
    : t("unit.m", { value: formatNumber(Math.round(m)) });
}

/**
 * A sentence about trips that are not completely on the server (`{count}` is the number of trips,
 * `{points}` the points of route): the verb and the pronoun follow the number of trips.
 */
export function unsentSentence(key: PluralKey, unsent: { trips: number; points: number }): string {
  return tp(key, unsent.trips, { points: tp("unit.points", unsent.points) });
}

/** The distance the phone counts itself, in metres, as the same kind of text. */
export function formatMetres(m: number): string {
  return formatDistance(m.toFixed(2)) ?? t("unit.m", { value: formatNumber(0) });
}

/**
 * The checks the server stored for a trip (`seq_gap:3 low_accuracy:1`) as sentences. A flag the
 * app does not know is shown as it is, so nothing the server says is hidden.
 */
export function integrityLines(flags: string): string[] {
  const lines: string[] = [];
  for (const flag of flags.split(/\s+/)) {
    if (flag === "") continue;
    const [name, count] = flag.split(":");
    const n = Number(count);
    if (
      (name === "seq_gap" || name === "speed_jump" || name === "low_accuracy") &&
      Number.isInteger(n) &&
      n > 0
    ) {
      lines.push(tp(`trip.check.${name}`, n));
    } else {
      lines.push(flag);
    }
  }
  return lines;
}

/** Whether the driver may submit the trip: the server has it ended and it was not submitted. */
export function canSubmit(trip: LocalTrip): boolean {
  if (trip.origin === "device" && !trip.endDone) return false;
  if (trip.submitDone || trip.submitRequested) return false;
  if (trip.discardReason !== null) return false;
  return (trip.serverState ?? "ended") === "ended";
}

/** Whether the driver may discard the trip: the server allows it while recording, ended or rejected, and a trip that a refusal stopped can always be cleared. */
export function canDiscard(trip: LocalTrip): boolean {
  if (trip.discardReason !== null || trip.discardDone) return false;
  // A trip stopped by a refusal (the server discarded it, or has lost it) can always be cleared
  // from the phone: the engine finishes a discard locally when the server says the trip is gone.
  if (trip.problem !== null && trip.origin === "device") return true;
  if (trip.submitRequested && !trip.submitDone) return false;
  const state = trip.serverState;
  if (state === null) return trip.origin === "device";
  return state === "recording" || state === "ended" || state === "rejected";
}
