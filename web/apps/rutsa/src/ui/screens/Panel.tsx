import { Button, Screen, Text } from "@obop/ui-native";
import { useEffect, useState } from "react";
import { View } from "react-native";
import { lastFixOfSegment, recordingHealth, type Health } from "../../core/liveness.ts";
import { formatKm } from "../../core/present.ts";
import { TripAlreadyActive, TripNotFound, TripStateError } from "../../core/repository.ts";
import type { Trip } from "../../core/types.ts";
import { t, tp } from "../../i18n/index.ts";
import { CannotRecord, pauseTrip, resumeTrip, stopTrip } from "../../native/actions.ts";
import { onChange } from "../../native/bus.ts";
import { isRecording } from "../../native/recorder.ts";
import { databaseInfo, getStore } from "../../native/store.ts";
import type { Check } from "../hooks.ts";

/**
 * TEMPORARY: the panel that drives a recording on the emulator until the screen of the recording
 * (D4) replaces it; since D3 it is the recording route, and Start is on the home screen. Its words
 * are not in the catalogue, since it goes away.
 */

/** One line that shows the database on the phone opened, encrypted, with its number of trips. */
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

/** What went wrong, for the panel. */
function problem(error: unknown): string {
  if (error instanceof CannotRecord) return `Cannot record: ${error.reason}`;
  if (error instanceof TripAlreadyActive) return t("error.alreadyActive");
  if (error instanceof TripStateError) return t("error.wrongState");
  if (error instanceof TripNotFound) return t("error.notFound");
  return `${t("error.generic")} (${error instanceof Error ? error.message : String(error)})`;
}

/** The time of day on the phone's clock. */
const timeOfDay = (at: number) => new Date(at).toTimeString().slice(0, 8);

/** What the check of a recording found; `danger` when the driver must act. */
function checkLine(check: Check): { text: string; danger: boolean } {
  if (check.kind === "failed")
    return { text: `check failed: ${problem(check.error)}`, danger: true };
  const { ensured } = check;
  switch (ensured.kind) {
    case "none":
      return { text: "check: no open trip", danger: false };
    case "paused":
      return { text: "check: paused, updates off", danger: false };
    case "just_started":
      return { text: "check: just started, left as it is", danger: false };
    case "restarted":
      return { text: `check: recording started again at ${timeOfDay(check.at)}`, danger: false };
    case "blocked":
      return { text: `check: cannot record: ${ensured.reason}`, danger: true };
  }
}

function healthText(health: Health): string {
  return health.kind === "quiet" ? `health: quiet ${health.minutes} min` : `health: ${health.kind}`;
}

/**
 * Start, Pause, Resume and Stop, the state of the open trip (or the last one) with its number of
 * points, the health of a recording, and what the check of the recording found (it runs at the
 * level of the app, `useRecordingCheck`). Every 5 s the health is computed again and the system is
 * asked whether the updates are still registered.
 */
function RecordingPanel({ check }: { check: Check | null }) {
  const [trip, setTrip] = useState<Trip | null>(null);
  const [points, setPoints] = useState(0);
  // `undefined` until the positions of the open trip have been read.
  const [lastFixAt, setLastFixAt] = useState<number | null | undefined>(undefined);
  const [now, setNow] = useState(Date.now);
  const [registered, setRegistered] = useState<boolean | null>(null);
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
  const line = check === null ? null : checkLine(check);

  return (
    <View style={{ gap: 12 }}>
      <Text testID="recording-status">{status}</Text>
      {health === null ? null : (
        <Text tone={healthy ? "muted" : "danger"} testID="recording-health">
          {`${healthText(health)} · service: ${service}`}
        </Text>
      )}
      {line === null ? null : (
        <Text tone={line.danger ? "danger" : "muted"} testID="recording-check">
          {line.text}
        </Text>
      )}
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

/** The screen of the recording route until D4; Start is on the home screen since D3. */
export function Panel({ check, onBack }: { check: Check | null; onBack: () => void }) {
  return (
    <Screen title="Recording" onBack={onBack}>
      <DatabaseLine />
      <RecordingPanel check={check} />
    </Screen>
  );
}
