import { Banner, Button, Screen, Text, useTheme, type BannerTone } from "@obop/ui-native";
import { useCallback, useEffect, useRef, useState } from "react";
import { Alert, View, type HostInstance } from "react-native";
import { recordingHealth, type Readiness } from "../../core/liveness.ts";
import { formatKm, phoneTimeZone, totals } from "../../core/present.ts";
import type { TripRepository } from "../../core/repository.ts";
import { LANGUAGE, t, tp, type MessageKey } from "../../i18n/index.ts";
import {
  CannotRecord,
  ensureRecording,
  pauseTrip,
  restartTrip,
  resumeTrip,
  stopTrip,
} from "../../native/actions.ts";
import { startWhenReady } from "../../native/start.ts";
import { announce } from "../announce.ts";
import { CurrentWidget } from "../CurrentWidget.tsx";
import { RecentTrips } from "../RecentTrips.tsx";
import { ROUND_MAX, ROUND_MIN, RoundButton } from "../RoundButton.tsx";
import { TripNameInput } from "../TripNameInput.tsx";
import { widgetValues } from "../widget-values.ts";
import { ScreenHeader } from "../ScreenHeader.tsx";
import { lastEndedTrip } from "../home.ts";
import { useNow, useProgress, useRegistered, useTrips, type Check } from "../hooks.ts";
import { tOs } from "../os-text.ts";
import { buttonsFor, warningOf, type Warning } from "../recording-view.ts";
import type { Nav } from "../router.ts";
import { endedNewestFirst, recentLine } from "../trips-list.ts";

// Read once in this start of the app: the note comes back only with another replaced file.
let restoredNoteRead = false;

type Blocked = Exclude<Readiness, "ready">;

/** Five recent trips at most, and the round button between its smallest and its largest. */
const ROOM = { maxRows: 5, minButton: ROUND_MIN, maxButton: ROUND_MAX } as const;
// The size the round button had, for the next drawing of the screen.
let lastRound: number = ROUND_MIN;

/** Which button's action runs: only that one shows it. */
type Busy = "main" | "side" | "again" | "restart" | null;

/**
 * The width of a small button at a side of the round one (Pause or Stop at the left, Restart at
 * the right), the same for each, so nothing moves when one comes, goes or changes.
 */
const SIDE_WIDTH = 100;

const BLOCKED_TEXT: Readonly<Record<Blocked, MessageKey>> = {
  services_off: "recording.blocked.services_off",
  no_permission: "recording.blocked.no_permission",
  approximate: "recording.blocked.approximate",
};

/** The words, the tone and the button of a warning (`warningOf`). */
function warningBanner(
  warning: Warning,
  actions: { restart: () => void; fix: () => void },
): { tone: BannerTone; title: string; action?: { label: string; onPress: () => void } } {
  const restart = { label: t("recording.restart"), onPress: actions.restart };
  switch (warning.kind) {
    case "action_failed":
      return { tone: "danger", title: t("error.generic") };
    case "restart_failed":
      return { tone: "danger", title: t("recording.restart.failed"), action: restart };
    case "blocked":
      return {
        tone: "danger",
        title: t(BLOCKED_TEXT[warning.reason]),
        action: { label: t("recording.blocked.action"), onPress: actions.fix },
      };
    case "stuck":
      // No button: the trip has Restart under the widget, and the app starts the updates again
      // by itself when it comes back to the front.
      return { tone: "danger", title: t("recording.stuck") };
    case "no_fix":
      return { tone: "danger", title: tOs("recording.nofix"), action: restart };
    case "quiet":
      // No button: standing still is normal, and the trip has Restart under the widget.
      return { tone: "warning", title: tp("recording.quiet", warning.minutes) };
    case "signal":
      return {
        tone: "warning",
        title: t(warning.note.kind === "weak" ? "recording.weak" : "recording.jumpy"),
      };
  }
}

/**
 * The first screen, the tab Record, and the screen of a recording (D4): the big button goes Start, then a red Stop
 * with Pause at its left, then Resume with Stop at its left while paused; Stop asks first. While a
 * trip is open it shows on top: Recording or Paused and its name ("Untitled trip 33" until it is
 * named in History), its elapsed time, its km so far (the same count Stop makes) and its start,
 * above the totals of today and this month, which leave the open trip out until it ends. The one
 * warning of a recording that matters most (`warningOf`) has its way on: Restart recording, or the
 * way to the permissions. The totals follow the phone's day.
 *
 * After `Home` and `Recording` of `mileage-app` (`src/ui/screens/`), on one screen, with Pause and
 * Resume, without the purpose of stage 2 and what talks to its server.
 */
export function Home({
  store,
  nav,
  recoveredStaleFile,
  check,
}: {
  store: TripRepository;
  nav: Nav;
  recoveredStaleFile: boolean;
  check: Check | null;
}) {
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
  // The size of the round button: what the recent trips leave (`RecentTrips`), kept for the
  // next time the screen is drawn so it does not start small.
  const [round, setRoundState] = useState(lastRound);
  // A change under a point is none: the round button does not move for a rounding.
  const setRound = useCallback((size: number) => {
    setRoundState((current) => {
      if (Math.abs(current - size) < 1) return current;
      lastRound = size;
      return size;
    });
  }, []);
  // The row of the round button, measured by the recent trips.
  const buttonRow = useRef<HostInstance>(null);
  // Not asked while a button's action runs: Stop stops the updates before it ends the trip, and
  // Resume opens the segment before it starts them, so an answer in between is a false "stopped".
  // Asked again at once when the action is done.
  const registered = useRegistered(active?.state === "recording" && busy === null);
  const [restoredRead, setRestoredRead] = useState(restoredNoteRead);
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
  const sum = totals(trips, now, zone);
  const words = { today: t("day.today"), yesterday: t("day.yesterday") };

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

  const shown =
    warning === null
      ? null
      : warningBanner(warning, {
          restart,
          fix: () => nav.push({ name: "permissions", next: "restart" }),
        });

  const mainButton = (() => {
    switch (buttons.main) {
      case "start":
        return (
          <RoundButton
            size={round}
            label={t("home.start")}
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
            size={round}
            label={t("home.stop")}
            danger
            hint={t("home.stop.hint")}
            busy={busy === "main"}
            onPress={() => active && confirmStop("main", active.id)}
            testID="stop"
          />
        );
      case "resume":
        return (
          <RoundButton
            size={round}
            label={t("home.resume")}
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
      <Button
        label={t("home.pause")}
        variant="secondary"
        busy={busy === "side"}
        onPress={() => act("side", () => pauseTrip(active.id))}
        testID="pause"
      />
    ) : (
      <Button
        label={t("home.stop")}
        variant="danger"
        busy={busy === "side"}
        onPress={() => confirmStop("side", active.id)}
        testID="stop-paused"
      />
    );

  const restartButton =
    active && active.state !== "ended" ? (
      <Button
        label={t("home.restart")}
        variant="secondary"
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

  return (
    <Screen
      title={t("app.title")}
      // "Record" in the middle and the pill Edit at the right (a PLACEHOLDER of stage 2: no tap
      // yet), as high as the other headers, so the current widget stands where it stands on the
      // trip's screen.
      titleSlot={
        <ScreenHeader
          title={t("tabs.record")}
          right={{
            text: t("header.edit"),
            label: t("header.edit"),
            inactive: true,
            testID: "record-edit",
          }}
        />
      }
    >
      {recoveredStaleFile && !restoredRead ? (
        <Banner
          tone="warning"
          title={t("home.restored.title")}
          message={t("home.restored.message")}
          action={{
            label: t("home.restored.action"),
            onPress: () => {
              restoredNoteRead = true;
              setRestoredRead(true);
            },
          }}
          testID="restored"
        />
      ) : null}

      {/* Always there: the open trip, else the last one that ended, else "No trips yet." */}
      <CurrentWidget
        trip={widgetTrip}
        {...(widgetTrip === null
          ? { elapsed: "", km: "", clock: "" }
          : widgetValues(widgetTrip, active ? progress : lastProgress, now, LANGUAGE, zone, words))}
        // The name is typed in place here too; a tap elsewhere on the card opens the trip.
        nameSlot={widgetTrip === null ? undefined : <TripNameInput trip={widgetTrip} />}
        onPress={() => widgetTrip && nav.push({ name: "trip", id: widgetTrip.id })}
      />

      {/* Under the widget: the round button in the middle; Restart at its left and Pause (or Stop
        while paused) at its right, centred on its height, a small gap from it, in places of the
        same width that stay when the buttons are not there, so nothing moves when they come or
        go. */}
      <View
        ref={buttonRow}
        style={{
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "center",
          gap: theme.space[3],
        }}
      >
        <View style={{ width: SIDE_WIDTH }}>{restartButton}</View>
        {mainButton}
        <View style={{ width: SIDE_WIDTH }}>{sideButton}</View>
      </View>

      {shown ? (
        <Banner
          tone={shown.tone}
          title={shown.title}
          {...(shown.action ? { action: shown.action } : {})}
          testID={`warning-${warning?.kind ?? ""}`}
        />
      ) : null}
      {warning?.kind === "signal" ? (
        <Text tone="muted" testID="signal-detail">
          {tp("recording.signal.detail", warning.note.dropped, { total: warning.note.stored })}
        </Text>
      ) : null}

      {/* The totals under a grey header, as Recent trips. */}
      <View style={{ gap: theme.space[2] }} testID="statistics">
        <Text variant="label" tone="muted" accessibilityRole="header">
          {t("home.statistics")}
        </Text>
        <View style={{ flexDirection: "row", gap: 24 }}>
          <View style={{ flex: 1 }}>
            <Text tone="muted">{t("home.today")}</Text>
            <Text variant="title" testID="today-km">
              {formatKm(sum.todayM, LANGUAGE)}
            </Text>
          </View>
          <View style={{ flex: 1 }}>
            <Text tone="muted">{t("home.month")}</Text>
            <Text variant="title" testID="month-km">
              {formatKm(sum.monthM, LANGUAGE)}
            </Text>
          </View>
        </View>
      </View>

      {/* The newest trips that ended, as many as fit above the switcher, and the way to History. */}
      <RecentTrips
        lines={endedNewestFirst(trips).map((trip) =>
          recentLine(trip, now, LANGUAGE, zone, {
            today: t("day.today"),
            yesterday: t("day.yesterday"),
            trips: (count) => tp("trips.count", count),
            manual: t("trips.manual"),
          }),
        )}
        buttonRowRef={buttonRow}
        limits={ROOM}
        onButton={setRound}
        onOpen={(id) => nav.push({ name: "trip", id })}
        onHistory={() => nav.tab("history")}
      />
    </Screen>
  );
}
