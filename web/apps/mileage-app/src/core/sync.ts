import type { DrainReport, Engine } from "./engine.ts";

export interface SyncStatus {
  /** A pass is running now. */
  running: boolean;
  /** The result of the last pass, or `null` before the first. */
  report: DrainReport | null;
  /** A bug or a broken database, not a network matter: the text of the error of the last pass. */
  error: string | null;
}

export interface SyncController {
  /** Run a pass now (or as soon as the one that is running has ended). */
  kick(): void;
  /**
   * Stop the timer. A pass that is running is not cancelled here: it stops itself before its next
   * call when the sign-in has changed (the engine's `identity`).
   */
  stop(): void;
  status(): SyncStatus;
  subscribe(listener: () => void): () => void;
}

export interface SyncOptions {
  engine: Engine;
  /**
   * The instance refused the access token. The session drops it so the next call refreshes; if
   * the refresh fails too, the session says so and the pass ends as signed out again.
   */
  onRejected: () => Promise<void>;
  /** Whether a sign-in exists to send with. */
  signedIn: () => boolean;
  /** Milliseconds between passes while there is a trip to send and nobody asked for another time. */
  idleMs?: number;
  /** Pass the timer functions in tests. */
  timers?: {
    set: (fn: () => void, ms: number) => unknown;
    clear: (handle: unknown) => void;
  };
}

export function createSyncController({
  engine,
  onRejected,
  signedIn,
  idleMs = 60_000,
  timers = {
    set: (fn, ms) => setTimeout(fn, ms),
    clear: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
  },
}: SyncOptions): SyncController {
  let status: SyncStatus = { running: false, report: null, error: null };
  let timer: unknown = null;
  let stopped = false;
  let again = false;
  const listeners = new Set<() => void>();

  const set = (next: Partial<SyncStatus>) => {
    status = { ...status, ...next };
    for (const l of listeners) l();
  };

  const schedule = (ms: number) => {
    if (stopped) return;
    if (timer !== null) timers.clear(timer);
    timer = timers.set(() => {
      timer = null;
      void run();
    }, ms);
  };

  async function pass(): Promise<DrainReport> {
    let report = await engine.drain();
    if (report.state === "signed_out") {
      // One try with a fresh token before the driver is asked to sign in.
      await onRejected();
      if (signedIn()) report = await engine.drain();
    }
    return report;
  }

  async function run(): Promise<void> {
    if (stopped) return;
    if (status.running) {
      again = true;
      return;
    }
    if (!signedIn()) {
      set({ report: null });
      return;
    }
    set({ running: true });
    try {
      const report = await pass();
      set({ running: false, report, error: null });
      if (report.state === "signed_out") return;
      if (report.state === "interrupted") {
        // The sign-in changed during the pass: look again under the new one (a signed-out phone
        // or a foreign outbox makes `signedIn()` false and nothing is sent).
        again = false;
        void run();
        return;
      }
      if (again) {
        again = false;
        void run();
        return;
      }
      if (report.pending > 0) schedule(report.retryInMs ?? idleMs);
    } catch (error) {
      set({ running: false, error: error instanceof Error ? error.message : String(error) });
      schedule(30_000);
    }
  }

  return {
    kick() {
      stopped = false;
      if (timer !== null) {
        timers.clear(timer);
        timer = null;
      }
      void run();
    },
    stop() {
      stopped = true;
      if (timer !== null) timers.clear(timer);
      timer = null;
    },
    status: () => status,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
