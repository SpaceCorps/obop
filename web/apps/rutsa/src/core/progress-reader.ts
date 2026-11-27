/**
 * The least time between two reads of the count of a trip, in milliseconds. A position is stored
 * about every 2 to 5 seconds; reading all of a trip's positions for each of them is wasted work.
 */
export const READ_EVERY_MS = 4000;

/** What the reader is wired to. */
export interface ProgressReaderPorts<T> {
  /** The open trip, `null` when there is none: then nothing is read and nothing is listened to. */
  id: string | null;
  /** The count of a trip (`TripRepository.progress`). */
  read: (id: string) => Promise<T>;
  /** Calls the listener whenever something was stored; answers the function that stops that (`onChange`). */
  subscribe: (listener: () => void) => () => void;
  /** The time now in milliseconds (`Date.now`). */
  clock: () => number;
  /** Receives each count. */
  set: (value: T) => void;
  /** Runs `run` once after `ms` milliseconds and answers what `clearTimeout` takes (`setTimeout`). */
  setTimeout: (run: () => void, ms: number) => unknown;
  /** Cancels what `setTimeout` answered, if it has not run yet (`clearTimeout`). */
  clearTimeout: (timer: unknown) => void;
  /** Overrides `READ_EVERY_MS`; the screen does not. */
  throttleMs?: number;
}

/**
 * Keeps the count of the open trip on the screen up to date: it reads at once, and again whenever
 * the store reports a change. Two reads are not closer than `READ_EVERY_MS`, and a change inside
 * that time is not lost: it is read when the time is up (one timer at most). A read that fails is
 * dropped without a word. Answers the function that stops it: after that nothing more is handed
 * to `set`, the timer is cancelled and the subscription is ended. No React in it, so it is tested.
 *
 * From `startProgressReader` of `mileage-app` (`src/core/progress-reader.ts`), for any count.
 */
export function startProgressReader<T>(ports: ProgressReaderPorts<T>): () => void {
  const { id, read, subscribe, clock, set } = ports;
  const throttleMs = ports.throttleMs ?? READ_EVERY_MS;
  if (id === null) return () => undefined;
  let alive = true;
  let last: number | null = null;
  // The read that waits for the end of the time, `null` when there is none. An object, so that a
  // timer which was cancelled but runs anyway can tell that it is not the current one.
  let waiting: { timer: unknown } | null = null;
  const cancelWaiting = () => {
    if (waiting === null) return;
    const { timer } = waiting;
    waiting = null;
    ports.clearTimeout(timer);
  };
  const readAt = (at: number) => {
    last = at;
    // An async step, so that a read which throws at once is a failed read like any other.
    Promise.resolve()
      .then(() => read(id))
      .then(
        (value) => {
          if (alive) set(value);
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
      cancelWaiting();
      readAt(at);
      return;
    }
    if (waiting !== null) return;
    // The rest of the time, and never more than the whole of it (a clock set back is not waited for).
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
