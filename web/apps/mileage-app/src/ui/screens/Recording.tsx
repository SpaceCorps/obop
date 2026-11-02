import { analyseTrack, type Fix } from "@obop/trip-core";
import { Banner, Button, Screen, Text } from "@obop/ui-native";
import { useCallback, useEffect, useRef, useState } from "react";
import { Alert, AppState } from "react-native";
import { recordingHealth, type Readiness } from "../../core/liveness.ts";
import { formatMetres } from "../../core/present.ts";
import { MAX_POINTS_PER_TRIP } from "../../core/types.ts";
import { clock } from "../../core/values.ts";
import { t, tp } from "../../i18n/index.ts";
import { resumeRecording, stopTrip } from "../../native/actions.ts";
import { onChange } from "../../native/bus.ts";
import { openSettings } from "../../native/permissions.ts";
import { isRecording } from "../../native/recorder.ts";
import type { Runtime } from "../../native/runtime.ts";
import { announce } from "../announce.ts";
import { useNow, useOutbox } from "../hooks.ts";
import type { Nav } from "../router.ts";

interface Progress {
  points: number;
  metres: number;
  /** When the newest stored position was taken (ms), or `null`. */
  lastFixMs: number | null;
}

/** The phone's own count of the trip so far: the same rules as the server (`trip-core`), for the preview only. */
async function progressOf(rt: Runtime, clientTripId: string): Promise<Progress> {
  const stored = await rt.store.points(clientTripId);
  const fixes: Fix[] = stored.map((p) => ({
    seq: p.seq,
    t: p.t,
    lat: Number(p.lat),
    lon: Number(p.lon),
    accuracyM: p.accuracyM === null ? null : Number(p.accuracyM),
  }));
  let lastFixMs: number | null = null;
  for (const p of stored) lastFixMs = lastFixMs === null ? p.t : Math.max(lastFixMs, p.t);
  return { points: stored.length, metres: analyseTrack(fixes).distanceM, lastFixMs };
}

/** A trip that was started this recently has just had its service started: no need to start it again. */
const JUST_STARTED_MS = 10_000;

/**
 * The recording screen: the clock, the phone's own count, and whether positions really arrive.
 * It is also what a driver sees when the sign-in ended during the trip (`signedOut`): the trip
 * keeps recording on the phone, Stop works without a connection, and a button goes to the sign-in.
 * The body scrolls, so large text never hides a banner or its button; Stop stays pinned.
 */
export function Recording({
  rt,
  nav,
  signedOut = false,
  onSignIn,
}: {
  rt: Runtime;
  nav: Nav;
  signedOut?: boolean;
  onSignIn?: () => void;
}) {
  const { active, loaded } = useOutbox(rt);
  const now = useNow();
  // `null` until the stored positions have been read once: nothing is said about them before.
  const [measured, setMeasured] = useState<Progress | null>(null);
  const progress: Progress = measured ?? { points: 0, metres: 0, lastFixMs: null };
  const [running, setRunning] = useState(true);
  const [blocked, setBlocked] = useState<Exclude<Readiness, "ready"> | null>(null);
  const [restartFailed, setRestartFailed] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [stopFailed, setStopFailed] = useState(false);
  const announcedStuck = useRef(false);

  const id = active?.clientTripId ?? null;
  const startedAt = active?.startedAt ?? null;

  // The count is read when a point arrives, and at most every few seconds.
  useEffect(() => {
    if (id === null) return;
    let alive = true;
    let last = 0;
    const read = () => {
      const at = Date.now();
      if (at - last < 4000) return;
      last = at;
      progressOf(rt, id).then(
        (p) => alive && setMeasured(p),
        () => undefined,
      );
    };
    last = 0;
    read();
    const off = onChange(read);
    return () => {
      alive = false;
      off();
    };
  }, [rt, id]);

  // Starts the service again when it is not running, and says what stands in the way when the
  // phone cannot record. The system does not restart the service after the process was restarted,
  // and the task stays registered without one, so "registered" proves nothing.
  const ensure = useCallback(async (): Promise<void> => {
    try {
      const ready = await resumeRecording();
      setBlocked(ready === "ready" ? null : ready);
      setRestartFailed(false);
      setRunning(ready === "ready" ? await isRecording() : false);
    } catch {
      setRestartFailed(true);
    }
  }, []);

  useEffect(() => {
    if (id === null) return;
    announce(t("announce.recording.started"));
  }, [id]);

  // On opening the screen and whenever the app comes back to the front.
  useEffect(() => {
    if (id === null || startedAt === null) return;
    if (Date.now() - Date.parse(startedAt) >= JUST_STARTED_MS) void ensure();
    const subscription = AppState.addEventListener("change", (state) => {
      if (state === "active") void ensure();
    });
    return () => subscription.remove();
  }, [id, startedAt, ensure]);

  // Whether the task is still registered: the system can stop it.
  useEffect(() => {
    let alive = true;
    const check = () =>
      isRecording().then(
        (r) => alive && setRunning(r),
        () => undefined,
      );
    const timer = setInterval(() => void check(), 5000);
    void check();
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (!running && !announcedStuck.current) {
      announcedStuck.current = true;
      announce(t("announce.recording.stuck"));
    }
    if (running) announcedStuck.current = false;
  }, [running]);

  // The trip is not open any more (stopped or discarded elsewhere): back to the home screen.
  useEffect(() => {
    if (loaded && active === null && !stopping) nav.home();
  }, [loaded, active, stopping, nav]);

  if (!active) {
    return (
      <Screen title={t("recording.title")} scroll={false}>
        <Text tone="muted">{t("common.loading")}</Text>
      </Screen>
    );
  }

  const health = recordingHealth({
    startedAtMs: Date.parse(active.startedAt),
    lastFixMs: measured === null ? undefined : measured.lastFixMs,
    nowMs: now,
  });

  const confirmStop = () => {
    Alert.alert(t("recording.stop.title"), t("recording.stop.message"), [
      { text: t("common.cancel"), style: "cancel" },
      {
        text: t("recording.stop"),
        style: "destructive",
        onPress: () => {
          setStopping(true);
          setStopFailed(false);
          stopTrip(active.clientTripId).then(
            () => {
              announce(t("announce.recording.stopped"));
              if (!signedOut) nav.replace({ name: "trip", id: active.clientTripId });
            },
            () => {
              setStopping(false);
              setStopFailed(true);
            },
          );
        },
      },
    ]);
  };

  const restart = { label: t("recording.stuck.action"), onPress: () => void ensure() };
  const settings = { label: t("permission.settings"), onPress: () => void openSettings() };

  return (
    <Screen
      title={t("recording.title")}
      footer={
        <Button
          label={t("recording.stop")}
          variant="danger"
          onPress={confirmStop}
          busy={stopping}
          testID="stop"
        />
      }
    >
      {signedOut ? (
        <Banner
          tone="warning"
          title={t("recording.signedout.title")}
          message={t("recording.signedout.message")}
          {...(onSignIn
            ? { action: { label: t("recording.signedout.action"), onPress: onSignIn } }
            : {})}
          testID="signedout-banner"
        />
      ) : null}
      {stopFailed ? (
        <Banner tone="danger" title={t("recording.stop.failed")} testID="stop-failed" />
      ) : null}
      {blocked === "services_off" ? (
        <Banner
          tone="danger"
          title={t("recording.services.title")}
          message={t("recording.services.message")}
          action={restart}
          testID="blocked-services"
        />
      ) : null}
      {blocked === "approximate" ? (
        <Banner
          tone="danger"
          title={t("recording.precise.title")}
          message={t("recording.precise.message")}
          action={settings}
          testID="blocked-precise"
        />
      ) : null}
      {blocked === "no_permission" ? (
        <Banner
          tone="danger"
          title={t("recording.permission.title")}
          message={t("recording.permission.message")}
          action={settings}
          testID="blocked-permission"
        />
      ) : null}
      {!running && blocked === null ? (
        <Banner
          tone="danger"
          title={t("recording.stuck.title")}
          message={t("recording.stuck.message")}
          action={restart}
          testID="stuck"
        />
      ) : null}
      {restartFailed ? (
        <Banner tone="danger" title={t("recording.error.start")} testID="restart-failed" />
      ) : null}
      {health.kind === "no_fix" && blocked === null ? (
        <Banner
          tone="danger"
          title={t("recording.nofix.title")}
          message={t("recording.nofix.message")}
          action={restart}
          testID="no-fix"
        />
      ) : null}
      {health.kind === "quiet" && blocked === null ? (
        <Banner
          tone="warning"
          title={t("recording.quiet.title")}
          message={tp("recording.quiet.message", health.minutes)}
          action={restart}
          testID="quiet"
        />
      ) : null}
      {progress.points >= MAX_POINTS_PER_TRIP ? (
        <Banner
          tone="warning"
          title={t("recording.full.title")}
          message={t("recording.full.message")}
        />
      ) : null}

      <Text variant="display" accessibilityRole="timer" testID="elapsed">
        {clock(now - Date.parse(active.startedAt))}
      </Text>
      <Text testID="distance">
        {t("recording.distance", { distance: formatMetres(progress.metres) })}
      </Text>
      <Text tone="muted" testID="points">
        {tp("recording.points", progress.points)}
      </Text>
      {measured !== null && progress.points === 0 && health.kind === "waiting" ? (
        <Text tone="muted">{t("recording.nofix")}</Text>
      ) : null}
      <Banner tone="info" title={t("recording.keep.title")} message={t("recording.keep.message")} />
    </Screen>
  );
}
