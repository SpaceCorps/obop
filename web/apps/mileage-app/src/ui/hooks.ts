import { useCallback, useEffect, useState } from "react";
import type { SessionStatus } from "../core/session.ts";
import type { SyncStatus } from "../core/sync.ts";
import type { LocalTrip } from "../core/types.ts";
import { onChange } from "../native/bus.ts";
import { getRuntime, type Runtime } from "../native/runtime.ts";

export type Loaded =
  | { state: "loading" }
  | { state: "ready"; runtime: Runtime }
  | { state: "failed"; message: string };

/** Opens the encrypted database and reads the session, once. */
export function useRuntime(): Loaded {
  const [loaded, setLoaded] = useState<Loaded>({ state: "loading" });
  useEffect(() => {
    let alive = true;
    getRuntime().then(
      (runtime) => alive && setLoaded({ state: "ready", runtime }),
      (error: unknown) =>
        alive &&
        setLoaded({ state: "failed", message: error instanceof Error ? error.message : "error" }),
    );
    return () => {
      alive = false;
    };
  }, []);
  return loaded;
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

export function useSessionStatus(rt: Runtime): SessionStatus {
  const [status, setStatus] = useState<SessionStatus>(rt.session.status());
  useEffect(() => {
    setStatus(rt.session.status());
    return rt.session.subscribe(() => setStatus(rt.session.status()));
  }, [rt]);
  return status;
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
