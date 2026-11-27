import { useTheme } from "@obop/ui-native";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Alert, View } from "react-native";
import { recordingHealth, type Readiness } from "../core/liveness.ts";
import { phoneTimeZone } from "../core/present.ts";
import type { TripRepository } from "../core/repository.ts";
import { LANGUAGE, t } from "../i18n/index.ts";
import {
  CannotRecord,
  ensureRecording,
  pauseTrip,
  restartTrip,
  resumeTrip,
  stopTrip,
} from "../native/actions.ts";
import { startWhenReady } from "../native/start.ts";
import { announce } from "./announce.ts";
import { lastEndedTrip } from "./home.ts";
import { useNow, useProgress, useRegistered, useTrips, type Check } from "./hooks.ts";
import { buttonsFor, warningOf, type Warning } from "./recording-view.ts";
import { RecordWidget } from "./RecordWidget.tsx";
import { ROUND_SIZE, RoundButton } from "./RoundButton.tsx";
import { ROUND_ICON_SIZE, RoundIconButton } from "./RoundIconButton.tsx";
import type { Nav } from "./router.ts";
import { TripNameInput } from "./TripNameInput.tsx";
import { widgetValues } from "./widget-values.ts";

export type Blocked = Exclude<Readiness, "ready">;

/** Which button's action runs: only that one shows it. */
type Busy = "main" | "side" | "again" | "restart" | null;

/**
 * The width of the column of the small round buttons at the left of the big one (Pause or Stop,
 * then Restart).
 */
const SIDE_WIDTH = ROUND_ICON_SIZE;

/**
 * The recording (D4), for the switcher at the bottom of the three tabs: the record widget with its
 * buttons (`widget`): the big button goes Start, then Stop with Pause and Restart at its left,
 * then Resume with Stop and Restart at its left while paused; Stop and Restart ask first. Also the
 * one warning of a recording that matters most (`warningOf`) and Restart recording, which the home
 * screen shows above its totals. It lives above the tabs (`App.tsx`), so the widget is the same on
 * every tab and a button's action goes on when the tab changes.
 *
 * After `Home` and `Recording` of `mileage-app` (`src/ui/screens/`), with Pause and Resume,
 * without the purpose of stage 2 and what talks to its server.
 */
export function useRecordPanel({
  store,
  nav,
  check,
  compact = false,
}: {
  store: TripRepository;
  nav: Nav;
  check: Check | null;
  /**
   * Only the buttons, in a row in the middle: the round one, Restart at its left and Pause (Stop
   * while paused) at its right, on the screen of the open trip, which shows its name, time and km.
   */
  compact?: boolean;
}): { widget: ReactNode; warning: Warning | null; restart: () => void } {
  const theme = useTheme();
  const { trips, active } = useTrips(store);
  const last = lastEndedTrip(trips);
  // The trip of the current widget: the open one, else the last that ended.
  const widgetTrip = active ?? last;
  const now = useNow(active ? 1000 : 30_000);
  const progress = useProgress(store, active?.id ?? null, active?.state ?? null);
  // The time of the last trip without its pauses, for the widget while no trip is open.
  const lastProgress = useProgress(
    store,
    active === null && last?.source === "recorded" ? last.id : null,
    last?.state ?? null,
  );
  const [busy, setBusy] = useState<Busy>(null);
  // Not asked while a button's action runs: Stop stops the updates before it ends the trip, and
  // Resume opens the segment before it starts them, so an answer in between is a false "stopped".
  // Asked again at once when the action is done.
  const registered = useRegistered(active?.state === "recording" && busy === null);
  const [actionFailed, setActionFailed] = useState(false);
  const [restartFailed, setRestartFailed] = useState(false);
  const [restartBlocked, setRestartBlocked] = useState<Blocked | null>(null);
  // A check of C5 older than this was answered here (a restart or a resume that worked).
  const [checkHandledAt, setCheckHandledAt] = useState(0);

  // Said to a screen reader: a trip that starts, a recording that stops by itself.
  const shownId = useRef<string | null>(null);
  useEffect(() => {
    const id = active?.id ?? null;
    if (id !== null && shownId.current === null) announce(t("announce.recording.started"));
    shownId.current = id;
  }, [active?.id]);
  useEffect(() => {
    if (registered === false) announce(t("announce.recording.stuck"));
  }, [registered]);
  // A new trip starts without the failures of the last one.
  useEffect(() => {
    setActionFailed(false);
    setRestartFailed(false);
    setRestartBlocked(null);
  }, [active?.id]);

  const zone = phoneTimeZone();

  const checkBlocked =
    check?.kind === "done" && check.ensured.kind === "blocked" && check.at > checkHandledAt
      ? check.ensured.reason
      : null;
  const health = active
    ? recordingHealth({
        segmentStartedAt: active.segmentStartedAt,
        lastFixAt: progress === null ? undefined : progress.lastFixAt,
        now,
      })
    : ({ kind: "waiting" } as const);
  const warning = warningOf({
    state: active?.state ?? null,
    actionFailed,
    restartFailed,
    blocked: restartBlocked ?? checkBlocked,
    registered,
    health,
    signal: progress?.signal ?? null,
  });
  const buttons = buttonsFor(active?.state ?? null);

  /**
   * Runs the action of a button; only that button shows that it works (`which`), so the other one
   * keeps its size and place. A failure the driver cannot fix is said once.
   */
  const act = (
    which: Busy,
    action: () => Promise<unknown>,
    onRefusal?: (e: CannotRecord) => void,
  ) => {
    setBusy(which);
    setActionFailed(false);
    action()
      .catch((e: unknown) => {
        if (e instanceof CannotRecord && onRefusal) onRefusal(e);
        else setActionFailed(true);
      })
      .finally(() => setBusy(null));
  };

  /**
   * Stop asks first, in a dialog of the system. The details of the trip are typed afterwards on its
   * screen (D6), not at Stop: the window of D4b and its switch went after they were tried.
   */
  /**
   * Restart asks first, as Stop: the open trip starts again from now, the same trip (its number
   * stays), its time and km so far cleared. A phone that cannot record goes to the permissions.
   */
  const confirmRestart = (trip: { id: string; state: "recording" | "paused" }) => {
    Alert.alert(t("home.restart.title"), t("home.restart.message"), [
      { text: t("common.cancel"), style: "cancel" },
      {
        text: t("home.restart"),
        style: "destructive",
        onPress: () =>
          act(
            "again",
            async () => {
              await restartTrip(trip.id);
              setCheckHandledAt(Date.now());
            },
            () =>
              nav.push({
                name: "permissions",
                next: trip.state === "paused" ? "resume" : "restart",
              }),
          ),
      },
    ]);
  };

  const confirmStop = (which: Busy, id: string) => {
    Alert.alert(t("home.stop.title"), t("home.stop.message"), [
      { text: t("common.cancel"), style: "cancel" },
      {
        text: t("home.stop"),
        style: "destructive",
        onPress: () =>
          act(which, async () => {
            await stopTrip(id);
            announce(t("announce.recording.stopped"));
          }),
      },
    ]);
  };

  const restart = () => {
    setBusy("restart");
    setRestartFailed(false);
    ensureRecording()
      .then(
        (ensured) => {
          if (ensured.kind === "blocked") {
            setRestartBlocked(ensured.reason);
          } else {
            setRestartBlocked(null);
            setCheckHandledAt(Date.now());
          }
        },
        () => setRestartFailed(true),
      )
      .finally(() => setBusy(null));
  };

  const mainButton = (() => {
    switch (buttons.main) {
      case "start":
        return (
          <RoundButton
            label={t("home.start")}
            shape="circle"
            hint={t("home.start.hint")}
            busy={busy === "main"}
            onPress={() =>
              act("main", async () => {
                // Straight to the trip when nothing stands in the way; else the screen of the
                // permissions, which shows what does.
                if ((await startWhenReady()) === null) {
                  nav.push({ name: "permissions", next: "start" });
                }
              })
            }
            testID="start"
          />
        );
      case "stop":
        return (
          <RoundButton
            label={t("home.stop")}
            shape="square"
            hint={t("home.stop.hint")}
            busy={busy === "main"}
            onPress={() => active && confirmStop("main", active.id)}
            testID="stop"
          />
        );
      case "resume":
        return (
          <RoundButton
            label={t("home.resume")}
            shape="circle"
            busy={busy === "main"}
            onPress={() =>
              active &&
              act(
                "main",
                async () => {
                  await resumeTrip(active.id);
                  setCheckHandledAt(Date.now());
                },
                () => nav.push({ name: "permissions", next: "resume" }),
              )
            }
            testID="resume"
          />
        );
    }
  })();

  const sideButton =
    buttons.side === null || !active ? null : buttons.side === "pause" ? (
      <RoundIconButton
        icon="pause"
        label={t("home.pause")}
        busy={busy === "side"}
        onPress={() => act("side", () => pauseTrip(active.id))}
        testID="pause"
      />
    ) : (
      <RoundIconButton
        icon="stop"
        label={t("home.stop")}
        danger
        busy={busy === "side"}
        onPress={() => confirmStop("side", active.id)}
        testID="stop-paused"
      />
    );

  const restartButton =
    active && active.state !== "ended" ? (
      <RoundIconButton
        icon="restart"
        label={t("home.restart")}
        busy={busy === "again"}
        onPress={() =>
          confirmRestart({
            id: active.id,
            state: active.state === "paused" ? "paused" : "recording",
          })
        }
        testID="restart-trip"
      />
    ) : null;

  const widget = compact ? (
    <View
      style={{
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: theme.space[5],
      }}
      testID="current-buttons"
    >
      {restartButton ?? <View style={{ width: ROUND_ICON_SIZE }} />}
      {mainButton}
      {sideButton ?? <View style={{ width: ROUND_ICON_SIZE }} />}
    </View>
  ) : (
    <RecordWidget
      trip={widgetTrip}
      {...(() => {
        if (widgetTrip === null) return { elapsed: "", km: "" };
        // No clock time in the switcher: the trip's screen has it.
        const { elapsed, km } = widgetValues(
          widgetTrip,
          active ? progress : lastProgress,
          now,
          LANGUAGE,
          zone,
        );
        return { elapsed, km };
      })()}
      // The name is typed in place here too; a tap elsewhere on the card opens the trip.
      nameSlot={widgetTrip === null ? undefined : <TripNameInput trip={widgetTrip} />}
      onPress={() => widgetTrip && nav.push({ name: "trip", id: widgetTrip.id })}
      controls={
        // Pause (Stop while paused) over Restart, small and round, as high as the round
        // button together, at its left; with no trip open there are none and the round
        // button stands in the middle of the place, which keeps its width, so the three
        // at its left do not move. As far from the right edge of the switcher as the round
        // button is from its top edge and from the tabs (`TabBar`).
        <View
          style={{
            width: SIDE_WIDTH + theme.space[2] + ROUND_SIZE,
            flexDirection: "row",
            alignItems: "center",
            justifyContent: restartButton === null ? "center" : "flex-start",
            gap: theme.space[2],
            marginRight: theme.space[5] - 1,
          }}
        >
          {restartButton === null ? null : (
            <View style={{ width: SIDE_WIDTH, gap: theme.space[2] }}>
              {sideButton ?? <View style={{ height: ROUND_ICON_SIZE }} />}
              {restartButton}
            </View>
          )}
          {mainButton}
        </View>
      }
    />
  );

  return { widget, warning, restart };
}
