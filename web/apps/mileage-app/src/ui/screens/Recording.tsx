import { analyseTrack, type Fix } from "@obop/trip-core";
import { Banner, Button, Screen, Text } from "@obop/ui-native";
import { useEffect, useState } from "react";
import { Alert } from "react-native";
import { formatMetres } from "../../core/present.ts";
import { MAX_POINTS_PER_TRIP } from "../../core/types.ts";
import { clock } from "../../core/values.ts";
import { t } from "../../i18n/index.ts";
import { resumeRecording, stopTrip } from "../../native/actions.ts";
import { onChange } from "../../native/bus.ts";
import { isRecording } from "../../native/recorder.ts";
import type { Runtime } from "../../native/runtime.ts";
import { useNow, useOutbox } from "../hooks.ts";
import type { Nav } from "../router.ts";

interface Progress {
  points: number;
  metres: number;
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
  return { points: stored.length, metres: analyseTrack(fixes).distanceM };
}

export function Recording({ rt, nav }: { rt: Runtime; nav: Nav }) {
  const { active, loaded } = useOutbox(rt);
  const now = useNow();
  const [progress, setProgress] = useState<Progress>({ points: 0, metres: 0 });
  const [running, setRunning] = useState(true);
  const [stopping, setStopping] = useState(false);

  const id = active?.clientTripId ?? null;

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
        (p) => alive && setProgress(p),
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

  // Whether the service still runs: the system can stop it.
  useEffect(() => {
    let alive = true;
    const check = () =>
      isRecording().then(
        (r) => alive && setRunning(r),
        () => undefined,
      );
    void check();
    const timer = setInterval(() => void check(), 5000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

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

  const confirmStop = () => {
    Alert.alert(t("recording.stop.title"), t("recording.stop.message"), [
      { text: t("common.cancel"), style: "cancel" },
      {
        text: t("recording.stop"),
        style: "destructive",
        onPress: () => {
          setStopping(true);
          void stopTrip(active.clientTripId).then(() =>
            nav.replace({ name: "trip", id: active.clientTripId }),
          );
        },
      },
    ]);
  };

  return (
    <Screen
      title={t("recording.title")}
      scroll={false}
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
      <Text variant="display" accessibilityRole="timer" testID="elapsed">
        {clock(now - Date.parse(active.startedAt))}
      </Text>
      <Text testID="distance">
        {t("recording.distance", { distance: formatMetres(progress.metres) })}
      </Text>
      <Text tone="muted" testID="points">
        {t("recording.points", { count: progress.points })}
      </Text>
      {progress.points === 0 ? <Text tone="muted">{t("recording.nofix")}</Text> : null}
      {!running ? (
        <Banner
          tone="danger"
          title={t("recording.stuck.title")}
          message={t("recording.stuck.message")}
          action={{
            label: t("recording.stuck.action"),
            onPress: () =>
              void resumeRecording().then(
                () => setRunning(true),
                () => undefined,
              ),
          }}
        />
      ) : null}
      {progress.points >= MAX_POINTS_PER_TRIP ? (
        <Banner
          tone="warning"
          title={t("recording.full.title")}
          message={t("recording.full.message")}
        />
      ) : null}
      <Banner tone="info" title={t("recording.keep.title")} message={t("recording.keep.message")} />
    </Screen>
  );
}
