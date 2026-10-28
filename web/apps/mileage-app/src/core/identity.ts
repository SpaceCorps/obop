import type { ServerConfig } from "./session.ts";
import { holdsUnsent, type OutboxStore } from "./store.ts";

/**
 * Whose trips the outbox holds. The outbox belongs to one person on one instance: the address the
 * app signed in to and the person that the instance reports as the owner of this phone's device
 * record. A sign-in as someone else, or at another address, never uploads, shows or deletes the
 * trips of the previous owner on its own: it finds them "foreign" and the driver decides
 * (docs/spec/mobile.md 8.7).
 *
 * The instance is identified by its canonical address because that is all the phone can know: the
 * instance's own database id is not exposed over HTTP (docs/ISSUES.md L39). So the same person at
 * a new address of the same instance (a new LAN address, a changed `OBOP_PUBLIC_URL`) is foreign
 * too, and the trips can be uploaded again by signing in at the old address.
 */
export interface Owner {
  origin: string;
  user: string;
}

const OWNER_KEY = "owner";

export function parseOwner(text: string | null): Owner | null {
  if (text === null) return null;
  try {
    const v: unknown = JSON.parse(text);
    if (
      typeof v === "object" &&
      v !== null &&
      typeof (v as Owner).origin === "string" &&
      typeof (v as Owner).user === "string"
    ) {
      return { origin: (v as Owner).origin, user: (v as Owner).user };
    }
  } catch {
    // Not ours: treated as no owner.
  }
  return null;
}

export const sameOwner = (a: Owner, b: Owner): boolean =>
  a.origin === b.origin && a.user === b.user;

/**
 * What a sign-out would delete: the trips that are not completely on the server (points not taken
 * over, a step still to send, or a refusal that stopped them), and the points those trips hold on
 * the phone. The points count includes points that were uploaded already when their trip is not
 * finished on the server.
 */
export interface Unsent {
  trips: number;
  points: number;
}

export async function unsentOf(store: OutboxStore): Promise<Unsent> {
  let trips = 0;
  let points = 0;
  for (const t of await store.listTrips()) {
    const n = await store.pointCount(t.clientTripId);
    if (holdsUnsent(t, n)) {
      trips += 1;
      points += n;
    }
  }
  return { trips, points };
}

export type Binding =
  /** The outbox belongs to this sign-in (or to nobody yet, and now does). */
  | { kind: "own" }
  /** Trips of another sign-in are on the phone: nothing is uploaded or shown until the driver decides. */
  | { kind: "foreign"; unsent: Unsent };

/**
 * Called when a sign-in has finished and before anything is uploaded. An outbox with no owner is
 * adopted (the phone has never had another sign-in; a build that kept no owner is treated the
 * same). An outbox of another owner that holds nothing unsent is emptied (what is left is a copy of
 * the other person's list of trips) and adopted. One that holds unsent trips stays untouched.
 */
export async function bindOwner(store: OutboxStore, current: Owner): Promise<Binding> {
  const stored = parseOwner(await store.getMeta(OWNER_KEY));
  if (stored === null || sameOwner(stored, current)) {
    await store.setMeta(OWNER_KEY, JSON.stringify(current));
    return { kind: "own" };
  }
  const unsent = await unsentOf(store);
  if (unsent.trips === 0) {
    await store.clearAll();
    await store.setMeta(OWNER_KEY, JSON.stringify(current));
    return { kind: "own" };
  }
  return { kind: "foreign", unsent };
}

/** The same decision when the app starts: the session says who is signed in, the outbox who owns it. */
export async function bindingOf(store: OutboxStore, config: ServerConfig | null): Promise<Binding> {
  if (config?.user === undefined) return { kind: "own" };
  const stored = parseOwner(await store.getMeta(OWNER_KEY));
  if (stored === null || sameOwner(stored, { origin: config.baseUrl, user: config.user })) {
    return { kind: "own" };
  }
  return { kind: "foreign", unsent: await unsentOf(store) };
}

/** The driver chose to delete the other sign-in's trips and go on: everything goes, the owner is the new one. */
export async function takeOver(store: OutboxStore, current: Owner): Promise<void> {
  await store.clearAll();
  await store.setMeta(OWNER_KEY, JSON.stringify(current));
}
