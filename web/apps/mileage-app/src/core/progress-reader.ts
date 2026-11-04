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
  /** Runs `run` once after `ms` milliseconds and returns what `clearTimeout` takes (`setTimeout`). */
  setTimeout: (run: () => void, ms: number) => unknown;
  /** Cancels what `setTimeout` returned, if it has not run yet (`clearTimeout`). */
  clearTimeout: (timer: unknown) => void;
  /** Overrides `READ_EVERY_MS`; the screen does not. */
  throttleMs?: number;
}

/**
 * Keeps the Recording screen's count of the trip (`progressFrom`, with the note about a distance of
 * 0 m) up to date: it reads the stored positions of the open trip at once, and again whenever the
 * store reports a change. Two reads are not closer than `READ_EVERY_MS` (a timer that runs a
 * millisecond early is not waited for again), and a change inside that time is not lost: it is read
 * when the time is up (one timer at most, however many changes come in before; a read that happens
 * sooner takes its place). So a change is on the screen at most
 * `READ_EVERY_MS` after it was stored (plus the time of the read), also when nothing is stored
 * after it. A read that fails is dropped without a word, and still counts for the time. Returns the
 * function that stops it: after that nothing more is handed to `set` (a read that was under way is
 * not), the timer is cancelled and the subscription is ended.
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
  // The read that waits for the end of the time, `null` when there is none. An object, so that a
  // timer which was cancelled but runs anyway (it was queued already) can tell that it is not the
  // current one.
  let waiting: { timer: unknown } | null = null;
  // An async function, so that a read which throws at once is a failed read like any other.
  const load = async (tripId: string): Promise<Progress> => progressFrom(await read(tripId));
  const cancelWaiting = () => {
    if (waiting === null) return;
    const { timer } = waiting;
    waiting = null;
    ports.clearTimeout(timer);
  };
  const readAt = (at: number) => {
    last = at;
    load(id).then(
      (progress) => {
        if (alive) set(progress);
      },
      () => undefined,
    );
  };
  const onTimer = (mine: { timer: unknown }) => () => {
    if (!alive || waiting !== mine) return;
    waiting = null;
    readAt(clock());
  };
  const readOrWait = () => {
    if (!alive) return;
    const at = clock();
    if (last === null || at - last >= throttleMs) {
      // A read now takes the place of one that was waiting (its timer is late).
      cancelWaiting();
      readAt(at);
      return;
    }
    if (waiting !== null) return;
    // The rest of the time, and never more than the whole of it: a clock that was set back is not
    // waited for.
    const rest = Math.min(throttleMs, throttleMs - (at - last));
    const mine: { timer: unknown } = { timer: null };
    waiting = mine;
    mine.timer = ports.setTimeout(onTimer(mine), rest);
  };
  readOrWait();
  const off = subscribe(readOrWait);
  return () => {
    alive = false;
    cancelWaiting();
    off();
  };
}
