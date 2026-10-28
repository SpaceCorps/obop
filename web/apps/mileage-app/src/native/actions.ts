import * as Crypto from "expo-crypto";
import { unsentOf, type Unsent } from "../core/identity.ts";
import type { Readiness } from "../core/liveness.ts";
import type { LocalTrip, Purpose } from "../core/types.ts";
import { instant, uuidV4 } from "../core/values.ts";
import { emitChange } from "./bus.ts";
import { locationReadiness } from "./permissions.ts";
import { ensureRecording, startRecording, stopRecording } from "./recorder.ts";
import { getRuntime, type Runtime } from "./runtime.ts";

/**
 * What the buttons do. Each action first changes the outbox (which is what makes it safe: the
 * phone remembers what the driver asked for, whatever happens to the connection next) and then
 * asks the sync controller to tell the server.
 */

/** The phone cannot record a trip yet; `reason` says what the driver has to change. */
export class CannotRecord extends Error {
  readonly reason: Exclude<Readiness, "ready">;
  constructor(reason: Exclude<Readiness, "ready">) {
    super(`cannot record: ${reason}`);
    this.name = "CannotRecord";
    this.reason = reason;
  }
}

/**
 * Start: checks that the phone can record (precise location allowed, location switched on), stores
 * the trip, then starts the foreground service. If the service cannot start, the trip is removed again.
 */
export async function startTrip(purpose: Purpose): Promise<LocalTrip> {
  const rt = await getRuntime();
  const ready = await locationReadiness();
  if (ready !== "ready") throw new CannotRecord(ready);
  const id = uuidV4(await Crypto.getRandomBytesAsync(16));
  const now = Date.now();
  const trip = await rt.store.startTrip({
    clientTripId: id,
    purpose,
    startedAt: instant(now),
    nowMs: now,
  });
  try {
    await startRecording();
  } catch (error) {
    await rt.store.deleteTrip(id);
    emitChange();
    throw error;
  }
  emitChange();
  rt.sync.kick();
  return trip;
}

/**
 * The service may not be running although a trip is open (the system stopped it, the process
 * restarted, location was switched off): check what is needed and start it again. Answers what
 * stands in the way, if anything.
 */
export async function resumeRecording(): Promise<Readiness> {
  const ready = await ensureRecording();
  emitChange();
  return ready;
}

/** Stops the location service (it may be gone already) and ends the trip that is open, on the phone. */
async function stopOpenTrip(rt: Runtime): Promise<void> {
  try {
    await stopRecording();
  } catch {
    // The service may be gone already; the trip is stopped on the phone either way.
  }
  const active = await rt.store.activeTrip();
  if (active) await rt.store.stopTrip(active.clientTripId, instant(Date.now()));
}

/** Stop: no further points, the counts are fixed, then everything is sent. Works without a connection and without a sign-in. */
export async function stopTrip(clientTripId: string): Promise<void> {
  const rt = await getRuntime();
  try {
    await stopRecording();
  } catch {
    // The service may be gone already; the trip is stopped on the phone either way.
  }
  await rt.store.stopTrip(clientTripId, instant(Date.now()));
  emitChange();
  rt.sync.kick();
}

export async function submitTrip(clientTripId: string): Promise<void> {
  const rt = await getRuntime();
  await rt.store.requestSubmit(clientTripId);
  emitChange();
  rt.sync.kick();
}

/** Discard with a reason (the server needs one). Stops the recording first if the trip is still open. */
export async function discardTrip(clientTripId: string, reason: string): Promise<void> {
  const rt = await getRuntime();
  const active = await rt.store.activeTrip();
  if (active?.clientTripId === clientTripId) {
    try {
      await stopRecording();
    } catch {
      // As in stopTrip.
    }
  }
  await rt.store.requestDiscard(clientTripId, reason.trim(), instant(Date.now()));
  emitChange();
  rt.sync.kick();
}

/** The driver asks to try a refused step again. */
export async function retryTrip(clientTripId: string): Promise<void> {
  const rt = await getRuntime();
  await rt.store.clearProblem(clientTripId);
  emitChange();
  rt.sync.kick();
}

/**
 * The trips that exist only on this phone, and their points: the server does not hold the route
 * yet. A trip that a refusal stopped is counted, because its points are still only here.
 */
export async function unsentSummary(): Promise<Unsent> {
  const rt = await getRuntime();
  return unsentOf(rt.store);
}

/**
 * The end of a sign-in on this phone, after the instance has been told (or could not be).
 * The trip that is open is stopped on the phone. The trips that are not uploaded are kept for the
 * next sign-in of the same person at the same address (`keepUnsent`, the outbox stays bound to its
 * owner and nobody else uploads or sees it) or deleted; everything that the server holds in full
 * is a copy and goes.
 */
export async function leaveSession(rt: Runtime, keepUnsent: boolean): Promise<void> {
  rt.sync.stop();
  await stopOpenTrip(rt);
  const unsent = await unsentOf(rt.store);
  await rt.session.signOut();
  if (!keepUnsent || unsent.trips === 0) await rt.store.clearAll();
  emitChange();
}

/**
 * Sign out. The tokens leave the Keystore at once; the instance is told in the background of the
 * same call but never waited for longer than a few seconds, and when it could not be reached the
 * sign-in screen says so once. `keepUnsent` keeps the trips that are only on the phone for the
 * same person's next sign-in; otherwise they are deleted. The caller has asked the driver.
 */
export async function signOut(keepUnsent: boolean): Promise<void> {
  const rt = await getRuntime();
  const told = rt.revokeSignIn(); // reads the refresh token now, before it is forgotten
  await leaveSession(rt, keepUnsent);
  rt.setSignOutUntold(!(await told));
  emitChange();
}
