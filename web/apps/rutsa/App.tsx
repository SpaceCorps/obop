import { Button, InsetsProvider, Screen, Text, ThemeProvider } from "@obop/ui-native";
import { StatusBar } from "expo-status-bar";
import { useEffect, useState, type ReactNode } from "react";
import { AppState, View } from "react-native";
import { SafeAreaProvider, useSafeAreaInsets } from "react-native-safe-area-context";
import { lastFixOfSegment, recordingHealth, type Health } from "./src/core/liveness.ts";
import { formatKm } from "./src/core/present.ts";
import type { Ensured } from "./src/core/recording.ts";
import { TripAlreadyActive, TripNotFound, TripStateError } from "./src/core/repository.ts";
import type { Trip } from "./src/core/types.ts";
import { t, tp } from "./src/i18n/index.ts";
import {
  CannotRecord,
  ensureRecording,
  pauseTrip,
  resumeTrip,
  startTrip,
  stopTrip,
} from "./src/native/actions.ts";
import { onChange } from "./src/native/bus.ts";
import { locationAccess, requestLocation, requestNotifications } from "./src/native/permissions.ts";
import { isRecording } from "./src/native/recorder.ts";
import { databaseInfo, getStore } from "./src/native/store.ts";

/** Hands the space of the system bars to the components of `@obop/ui-native`. */
function Insets({ children }: { children: ReactNode }) {
  const insets = useSafeAreaInsets();
  return <InsetsProvider insets={insets}>{children}</InsetsProvider>;
}

/**
 * For now: one line that shows the database on the phone opened, encrypted, with its number of
 * trips. The screens of the prototype (block D of the plan) replace it.
 */
function DatabaseLine() {
  const [line, setLine] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    const load = () =>
      (async () => {
        const trips = await (await getStore()).listTrips();
        const { cipherVersion } = await databaseInfo();
        return `${tp("trips.count", trips.length)} · SQLCipher ${cipherVersion}`;
      })().then(
        (text) => live && setLine(text),
        () => live && setLine(t("error.database")),
      );
    void load();
    const off = onChange(() => void load());
    return () => {
      live = false;
      off();
    };
  }, []);
  return line === null ? null : <Text tone="muted">{line}</Text>;
}

/** What went wrong, for the panel below. */
function problem(error: unknown): string {
  if (error instanceof CannotRecord) return `Cannot record: ${error.reason}`;
  if (error instanceof TripAlreadyActive) return t("error.alreadyActive");
  if (error instanceof TripStateError) return t("error.wrongState");
  if (error instanceof TripNotFound) return t("error.notFound");
  return `${t("error.generic")} (${error instanceof Error ? error.message : String(error)})`;
}

/**
 * Start asks for the permissions itself while the permissions screen (D2) does not exist: the
 * location dialog when it can still be shown, and the notification permission of Android 13.
 */
async function startWithPermissions(): Promise<void> {
  if ((await locationAccess()) === "ask") await requestLocation();
  await requestNotifications();
  await startTrip();
}

/** The time of day on the phone's clock, for the panel below. */
const timeOfDay = (at: number) => new Date(at).toTimeString().slice(0, 8);

/** What the check of a recording found, for the panel below; `danger` when the driver must act. */
function checkLine(ensured: Ensured, at: number): { text: string; danger: boolean } {
  switch (ensured.kind) {
    case "none":
      return { text: "check: no open trip", danger: false };
    case "paused":
      return { text: "check: paused, updates off", danger: false };
    case "just_started":
      return { text: "check: just started, left as it is", danger: false };
    case "restarted":
      return { text: `check: recording started again at ${timeOfDay(at)}`, danger: false };
    case "blocked":
      return { text: `check: cannot record: ${ensured.reason}`, danger: true };
  }
}

function healthText(health: Health): string {
  return health.kind === "quiet" ? `health: quiet ${health.minutes} min` : `health: ${health.kind}`;
}

/**
 * For now: Start, Pause, Resume and Stop, the state of the open trip (or the last one) with its
 * number of points, and the health of a recording, so a recording can be tried on the emulator
 * before the screens of block D, which replace this panel. Its words are not in the catalogue,
 * since it goes away.
 *
 * When the app opens and whenever it comes back to the front, the recording is checked and started
 * again if Android ended the app's process (`ensureRecording`). Every 5 s the health is computed
 * again and the system is asked whether the updates are still registered.
 */
function RecordingPanel() {
  const [trip, setTrip] = useState<Trip | null>(null);
  const [points, setPoints] = useState(0);
  // `undefined` until the positions of the open trip have been read.
  const [lastFixAt, setLastFixAt] = useState<number | null | undefined>(undefined);
  const [now, setNow] = useState(Date.now);
  const [registered, setRegistered] = useState<boolean | null>(null);
  const [check, setCheck] = useState<{ text: string; danger: boolean } | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    const load = async () => {
      const store = await getStore();
      const shown = (await store.activeTrip()) ?? (await store.listTrips())[0] ?? null;
      const stored = shown === null ? [] : await store.points(shown.id);
      if (live) {
        setTrip(shown);
        setPoints(stored.length);
        setLastFixAt(shown === null ? null : lastFixOfSegment(stored, shown.segment));
      }
    };
    const reload = () => {
      load().catch(() => live && setMessage(t("error.database")));
    };
    reload();
    const off = onChange(reload);
    return () => {
      live = false;
      off();
    };
  }, []);

  // The check of the recording, at the start of the app and whenever it comes back to the front.
  useEffect(() => {
    let live = true;
    const run = () => {
      ensureRecording().then(
        (ensured) => live && setCheck(checkLine(ensured, Date.now())),
        (error: unknown) =>
          live && setCheck({ text: `check failed: ${problem(error)}`, danger: true }),
      );
    };
    run();
    const subscription = AppState.addEventListener("change", (state) => {
      if (state === "active") run();
    });
    return () => {
      live = false;
      subscription.remove();
    };
  }, []);

  // The clock of the health, and whether the updates are still registered with the system.
  useEffect(() => {
    let live = true;
    const tick = () => {
      setNow(Date.now());
      isRecording().then(
        (r) => live && setRegistered(r),
        () => live && setRegistered(null),
      );
    };
    tick();
    const timer = setInterval(tick, 5000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, []);

  const act = (action: () => Promise<unknown>) => {
    setBusy(true);
    setMessage(null);
    action()
      .catch((error: unknown) => setMessage(problem(error)))
      .finally(() => setBusy(false));
  };

  const status =
    trip === null
      ? "No trip yet"
      : `${trip.state} · ${points} points` +
        (trip.distanceM === null ? "" : ` · ${formatKm(trip.distanceM, "en")}`);

  const recording = trip?.state === "recording" ? trip : null;
  const health =
    recording === null
      ? null
      : recordingHealth({ segmentStartedAt: recording.segmentStartedAt, lastFixAt, now });
  const service = registered === null ? "unknown" : registered ? "on" : "off";
  // A recording whose updates are not registered any more records nothing.
  const healthy =
    health !== null && (health.kind === "ok" || health.kind === "waiting") && registered !== false;

  return (
    <View style={{ gap: 12 }}>
      <Text testID="recording-status">{status}</Text>
      {health === null ? null : (
        <Text tone={healthy ? "muted" : "danger"} testID="recording-health">
          {`${healthText(health)} · service: ${service}`}
        </Text>
      )}
      {check === null ? null : (
        <Text tone={check.danger ? "danger" : "muted"} testID="recording-check">
          {check.text}
        </Text>
      )}
      {trip === null || trip.state === "ended" ? (
        <Button label="Start" busy={busy} onPress={() => act(startWithPermissions)} />
      ) : null}
      {trip?.state === "recording" ? (
        <Button
          label="Pause"
          variant="secondary"
          busy={busy}
          onPress={() => act(() => pauseTrip(trip.id))}
        />
      ) : null}
      {trip?.state === "paused" ? (
        <Button label="Resume" busy={busy} onPress={() => act(() => resumeTrip(trip.id))} />
      ) : null}
      {trip !== null && trip.state !== "ended" ? (
        <Button
          label="Stop"
          variant="danger"
          busy={busy}
          onPress={() => act(() => stopTrip(trip.id))}
        />
      ) : null}
      {message === null ? null : <Text tone="danger">{message}</Text>}
    </View>
  );
}

export default function App() {
  return (
    <SafeAreaProvider>
      <ThemeProvider>
        <Insets>
          <Screen title="Rutsa">
            <Text tone="muted">Mileage log</Text>
            <DatabaseLine />
            <RecordingPanel />
          </Screen>
          <StatusBar style="auto" />
        </Insets>
      </ThemeProvider>
    </SafeAreaProvider>
  );
}
