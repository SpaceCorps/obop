import { progressFrom, type Progress } from "./progress.ts";
import type { StoredPoint } from "./types.ts";

/**
 * The least time between two reads of the stored positions, in milliseconds. A point is stored
 * about every 2 seconds; reading all of a trip's points for each of them would be wasted work.
 */
export const READ_EVERY_MS = 4000;

/** What the reader is wired to. Each of them is one line in the Recording screen. */
export interface ProgressReaderPorts {
  /** The open trip, `null` when there is none: then nothing is read and nothing is listened to. */
  id: string | null;
  /** The stored positions of a trip (`rt.store.points`). */
  read: (id: string) => Promise<readonly StoredPoint[]>;
  /** Calls the listener whenever something was stored; returns the function that stops that (`onChange`). */
  subscribe: (listener: () => void) => () => void;
  /** The time now in milliseconds (`Date.now`). */
  clock: () => number;
  /** Receives each count (`setMeasured`). */
  set: (progress: Progress) => void;
  /** Overrides `READ_EVERY_MS`; the screen does not. */
  throttleMs?: number;
}

/**
 * Keeps the Recording screen's count of the trip (`progressFrom`, with the note about a distance of
 * 0 m) up to date: it reads the stored positions of the open trip at once, and again whenever the
 * store reports a change, but not before `READ_EVERY_MS` has passed since the last read (a change
 * inside that time is not read later: the next one is). A read that fails is dropped without a
 * word, and still counts for the time. Returns the function that stops it: after that nothing more
 * is handed to `set` (a read that was under way is not), and the subscription is ended.
 *
 * This is the only link between the stored points and what the screen says about them, so it has
 * no React in it and the screen only hands it its ports.
 */
export function startProgressReader(ports: ProgressReaderPorts): () => void {
  const { id, read, subscribe, clock, set } = ports;
  const throttleMs = ports.throttleMs ?? READ_EVERY_MS;
  if (id === null) return () => undefined;
  let alive = true;
  let last: number | null = null;
  // An async function, so that a read which throws at once is a failed read like any other.
  const load = async (tripId: string): Promise<Progress> => progressFrom(await read(tripId));
  const readNow = () => {
    const at = clock();
    if (last !== null && at - last < throttleMs) return;
    last = at;
    load(id).then(
      (progress) => {
        if (alive) set(progress);
      },
      () => undefined,
    );
  };
  readNow();
  const off = subscribe(readNow);
  return () => {
    alive = false;
    off();
  };
}
