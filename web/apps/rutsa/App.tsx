import { Button, InsetsProvider, Screen, Text, ThemeProvider } from "@obop/ui-native";
import { StatusBar } from "expo-status-bar";
import { useEffect, useState, type ReactNode } from "react";
import { View } from "react-native";
import { SafeAreaProvider, useSafeAreaInsets } from "react-native-safe-area-context";
import { formatKm } from "./src/core/present.ts";
import { TripAlreadyActive, TripNotFound, TripStateError } from "./src/core/repository.ts";
import type { Trip } from "./src/core/types.ts";
import { t, tp } from "./src/i18n/index.ts";
import { CannotRecord, pauseTrip, resumeTrip, startTrip, stopTrip } from "./src/native/actions.ts";
import { onChange } from "./src/native/bus.ts";
import { locationAccess, requestLocation, requestNotifications } from "./src/native/permissions.ts";
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

/**
 * For now: Start, Pause, Resume and Stop, and the state of the open trip (or the last one) with its
 * number of points, so a recording can be tried on the emulator before the screens of block D,
 * which replace this panel. Its words are not in the catalogue, since it goes away.
 */
function RecordingPanel() {
  const [trip, setTrip] = useState<Trip | null>(null);
  const [points, setPoints] = useState(0);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    const load = async () => {
      const store = await getStore();
      const shown = (await store.activeTrip()) ?? (await store.listTrips())[0] ?? null;
      const count = shown === null ? 0 : (await store.points(shown.id)).length;
      if (live) {
        setTrip(shown);
        setPoints(count);
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

  return (
    <View style={{ gap: 12 }}>
      <Text testID="recording-status">{status}</Text>
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
