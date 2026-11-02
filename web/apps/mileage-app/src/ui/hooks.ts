import { useCallback, useEffect, useState } from "react";
import type { Binding } from "../core/identity.ts";
import { dismissStaleFileNote, staleFileRecoveredAt } from "../core/open-outbox.ts";
import type { SessionStatus } from "../core/session.ts";
import type { SyncStatus } from "../core/sync.ts";
import type { LocalTrip } from "../core/types.ts";
import { onChange } from "../native/bus.ts";
import { getRuntime, type Runtime } from "../native/runtime.ts";

export type Loaded =
  | { state: "loading" }
  | { state: "ready"; runtime: Runtime }
  | { state: "failed"; message: string };

/**
 * Opens the encrypted database and reads the session. `retry` tries again after a failure (a
 * failed open is not remembered by `getRuntime`).
 */
export function useRuntime(): Loaded & { retry: () => void } {
  const [loaded, setLoaded] = useState<Loaded>({ state: "loading" });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let alive = true;
    setLoaded({ state: "loading" });
    getRuntime().then(
      (runtime) => alive && setLoaded({ state: "ready", runtime }),
      (error: unknown) =>
        alive &&
        setLoaded({ state: "failed", message: error instanceof Error ? error.message : "error" }),
    );
    return () => {
      alive = false;
    };
  }, [attempt]);
  return { ...loaded, retry: () => setAttempt((n) => n + 1) };
}

/** Whose trips the outbox holds, compared with the sign-in: `foreign` blocks every screen but one. */
export function useBinding(rt: Runtime): Binding {
  const [binding, setBinding] = useState<Binding>(rt.binding());
  useEffect(() => {
    setBinding(rt.binding());
    return onChange(() => setBinding(rt.binding()));
  }, [rt]);
  return binding;
}

export interface Outbox {
  trips: LocalTrip[];
  active: LocalTrip | null;
  loaded: boolean;
}

/** The trips on this phone; reads again whenever something in the outbox changes. */
export function useOutbox(rt: Runtime): Outbox {
  const [state, setState] = useState<Outbox>({ trips: [], active: null, loaded: false });
  const load = useCallback(async () => {
    const [trips, active] = await Promise.all([rt.store.listTrips(), rt.store.activeTrip()]);
    setState({ trips, active, loaded: true });
  }, [rt]);
  useEffect(() => {
    let alive = true;
    const run = () => {
      load().catch(() => {
        if (alive) setState((s) => ({ ...s, loaded: true }));
      });
    };
    run();
    const off = onChange(run);
    return () => {
      alive = false;
      off();
    };
  }, [load]);
  return state;
}

/**
 * Whether the app replaced the database file of a restored backup at this start (iOS only; the
 * note stays until the driver has read it), and the way to dismiss it.
 */
export function useStaleFileNote(rt: Runtime): { recoveredAt: string | null; dismiss: () => void } {
  const [recoveredAt, setRecoveredAt] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    staleFileRecoveredAt(rt.store).then(
      (value) => alive && setRecoveredAt(value),
      () => undefined,
    );
    return () => {
      alive = false;
    };
  }, [rt]);
  const dismiss = useCallback(() => {
    setRecoveredAt(null);
    void dismissStaleFileNote(rt.store).catch(() => undefined);
  }, [rt]);
  return { recoveredAt, dismiss };
}

export function useSessionStatus(rt: Runtime): SessionStatus {
  const [status, setStatus] = useState<SessionStatus>(rt.session.status());
  useEffect(() => {
    setStatus(rt.session.status());
    return rt.session.subscribe(() => setStatus(rt.session.status()));
  }, [rt]);
  return status;
}

/**
 * Whether the last sign-out could not tell the instance. Read again whenever something changes: the
 * answer of the instance comes after the sign-in screen is open (a sign-out waits up to five
 * seconds for it), so the flag is set while the screen is already there.
 */
export function useSignOutUntold(rt: Runtime): boolean {
  const [untold, setUntold] = useState<boolean>(rt.signOutUntold());
  useEffect(() => {
    setUntold(rt.signOutUntold());
    return onChange(() => setUntold(rt.signOutUntold()));
  }, [rt]);
  return untold;
}

export function useSyncStatus(rt: Runtime): SyncStatus {
  const [status, setStatus] = useState<SyncStatus>(rt.sync.status());
  useEffect(() => {
    setStatus(rt.sync.status());
    return rt.sync.subscribe(() => setStatus(rt.sync.status()));
  }, [rt]);
  return status;
}

/** A clock that ticks every second while mounted. */
export function useNow(intervalMs = 1000): number {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}
