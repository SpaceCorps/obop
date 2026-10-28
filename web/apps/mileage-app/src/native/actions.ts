import * as Crypto from "expo-crypto";
import { hasWork } from "../core/store.ts";
import type { LocalTrip, Purpose } from "../core/types.ts";
import { instant, uuidV4 } from "../core/values.ts";
import { emitChange } from "./bus.ts";
import { startRecording, stopRecording } from "./recorder.ts";
import { getRuntime } from "./runtime.ts";

/**
 * What the buttons do. Each action first changes the outbox (which is what makes it safe: the
 * phone remembers what the driver asked for, whatever happens to the connection next) and then
 * asks the sync controller to tell the server.
 */

/** Start: stores the trip, then starts the foreground service. If the service cannot start, the trip is removed again. */
export async function startTrip(purpose: Purpose): Promise<LocalTrip> {
  const rt = await getRuntime();
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

/** The service is not running although a trip is open (the system stopped it): start it again. */
export async function resumeRecording(): Promise<void> {
  await startRecording();
  emitChange();
}

/** Stop: no further points, the counts are fixed, then everything is sent. */
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

/** Trips of this phone that the server does not have in full yet. */
export async function unsentTrips(): Promise<LocalTrip[]> {
  const rt = await getRuntime();
  return (await rt.store.listTrips()).filter((t) => t.origin === "device" && hasWork(t));
}

/**
 * Sign out. The instance is told to end the sign-in (best effort); the tokens leave the Keystore;
 * and every trip on the phone is deleted, uploaded or not, because the next person to sign in
 * must not inherit another driver's trips or upload them as their own. The caller has already
 * asked the driver about trips that are not uploaded.
 */
export async function signOut(): Promise<void> {
  const rt = await getRuntime();
  rt.sync.stop();
  try {
    await stopRecording();
  } catch {
    // Nothing was running.
  }
  await rt.revokeSignIn();
  await rt.session.signOut();
  await rt.store.clearAll();
  emitChange();
}
