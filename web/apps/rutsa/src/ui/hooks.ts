import { useEffect, useState } from "react";
import { AppState } from "react-native";
import { databaseProblem, type DatabaseProblem } from "../core/open-db.ts";
import type { Ensured } from "../core/recording.ts";
import type { TripRepository } from "../core/repository.ts";
import type { Trip } from "../core/types.ts";
import { ensureRecording } from "../native/actions.ts";
import { onChange } from "../native/bus.ts";
import { locationReadiness, notificationsAllowed } from "../native/permissions.ts";
import { databaseInfo, getStore } from "../native/store.ts";
import { onForeground } from "./foreground.ts";
import type { PhoneStatus } from "./status.ts";

/**
 * The data the screens read, as React hooks. What they wrap is tested where it lives (`core/` and
 * `native/`); the hooks only hold it in the state of a screen and read it again when the bus says
 * that the trips changed.
 *
 * After `mileage-app` (`src/ui/hooks.ts`), without the session, the sync and the sign-in.
 */

export type Loaded =
  | { state: "loading" }
  | { state: "ready"; store: TripRepository; recoveredStaleFile: boolean }
  | { state: "failed"; message: string; problem: DatabaseProblem };

/**
 * Opens the encrypted database. `retry` tries again after a failure (a failed open is not
 * remembered by `getStore`), also after the trips were erased.
 */
export function useStore(): Loaded & { retry: () => void } {
  const [loaded, setLoaded] = useState<Loaded>({ state: "loading" });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let alive = true;
    setLoaded({ state: "loading" });
    Promise.all([getStore(), databaseInfo()]).then(
      ([store, info]) =>
        alive && setLoaded({ state: "ready", store, recoveredStaleFile: info.recoveredStaleFile }),
      (error: unknown) =>
        alive &&
        setLoaded({
          state: "failed",
          message: error instanceof Error ? error.message : String(error),
          problem: databaseProblem(error),
        }),
    );
    return () => {
      alive = false;
    };
  }, [attempt]);
  return { ...loaded, retry: () => setAttempt((n) => n + 1) };
}

export interface Trips {
  /** Newest first. */
  trips: Trip[];
  /** The trip that records or is paused. */
  active: Trip | null;
  /** The trips have been read at least once. */
  loaded: boolean;
}

/** The trips on this phone; read again whenever the bus says that they changed. */
export function useTrips(store: TripRepository): Trips {
  const [state, setState] = useState<Trips>({ trips: [], active: null, loaded: false });
  useEffect(() => {
    let alive = true;
    const load = () => {
      Promise.all([store.listTrips(), store.activeTrip()]).then(
        ([trips, active]) => alive && setState({ trips, active, loaded: true }),
        () => alive && setState((s) => ({ ...s, loaded: true })),
      );
    };
    load();
    const off = onChange(load);
    return () => {
      alive = false;
      off();
    };
  }, [store]);
  return state;
}

/** A clock that ticks while mounted, every second by default. */
export function useNow(intervalMs = 1000): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}

/**
 * What the phone says about recording (`ui/status.ts`): the readiness of the location and whether
 * notifications are allowed. Read now, on every return to the front (back from the settings), and
 * every 15 s, since location can be switched in the quick settings without leaving the app. `null`
 * until the first answer.
 */
export function usePhoneStatus(): PhoneStatus | null {
  const [status, setStatus] = useState<PhoneStatus | null>(null);
  useEffect(() => {
    let alive = true;
    const read = () => {
      Promise.all([locationReadiness(), notificationsAllowed()]).then(
        ([readiness, notifications]) =>
          alive &&
          setStatus((s) =>
            s?.readiness === readiness && s.notifications === notifications
              ? s
              : { readiness, notifications },
          ),
        () => undefined,
      );
    };
    const stop = onForeground(AppState, read);
    const timer = setInterval(read, 15_000);
    return () => {
      alive = false;
      stop();
      clearInterval(timer);
    };
  }, []);
  return status;
}

/** What the last check of the recording found (`Recording.ensure`), and when. */
export type Check =
  | { kind: "done"; ensured: Ensured; at: number }
  | { kind: "failed"; error: unknown; at: number };

/**
 * The check of a recording (C5): when the app opens and whenever it comes back to the front, a
 * trip that records gets its location updates started again if Android ended the app's process,
 * and updates left running without a recording trip are stopped. It runs at the level of the app,
 * whatever screen is open. `null` until the first check has answered.
 */
export function useRecordingCheck(): Check | null {
  const [check, setCheck] = useState<Check | null>(null);
  useEffect(() => {
    let alive = true;
    const stop = onForeground(AppState, () => {
      ensureRecording().then(
        (ensured) => alive && setCheck({ kind: "done", ensured, at: Date.now() }),
        (error: unknown) => alive && setCheck({ kind: "failed", error, at: Date.now() }),
      );
    });
    return () => {
      alive = false;
      stop();
    };
  }, []);
  return check;
}
