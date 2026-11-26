import { Banner, Screen, Text, useTheme, type BannerTone } from "@obop/ui-native";
import { useEffect, useRef, useState } from "react";
import { Alert, View, type HostInstance } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
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
import { RecordWidget } from "../RecordWidget.tsx";
import { RecentTrips } from "../RecentTrips.tsx";
import { ROUND_SIZE, RoundButton } from "../RoundButton.tsx";
import { TabBar } from "../TabBar.tsx";
import { ROUND_ICON_SIZE, RoundIconButton } from "../RoundIconButton.tsx";
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

/** Which button's action runs: only that one shows it. */
type Busy = "main" | "side" | "again" | "restart" | null;

/**
 * The width of the column of the small round buttons at the left of the big one (Pause or Stop,
 * then Restart).
 */
const SIDE_WIDTH = ROUND_ICON_SIZE;

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
  // The switcher with the record widget: the recent trips end above it.
  const footerRef = useRef<HostInstance>(null);
  // The bar of the system under the switcher (the insets of this tab keep room for the small one).
  const safeArea = useSafeAreaInsets();
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

  return (
    <View style={{ flex: 1 }}>
      <Screen
        title={t("app.title")}
        // "Record" in the middle and the pill Edit at the right (a PLACEHOLDER of stage 2: no tap yet).
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
          maxRows={5}
          footerRef={footerRef}
          onOpen={(id) => nav.push({ name: "trip", id })}
          onHistory={() => nav.tab("history")}
        />
      </Screen>
      {/* The switcher of Record, drawn here: it holds the record widget over its tabs (the other
      tabs have the small one of `App.tsx`), and the recent trips end above it. */}
      <TabBar
        tab="record"
        onTab={nav.tab}
        bottom={safeArea.bottom}
        barRef={footerRef}
        header={
          <>
            {/* Always there: the open trip, else the last one that ended, else "No trips yet." With the
        buttons of a recording at its right: the round one, Restart and Pause (Stop while
        paused) in a column at its left, a place of the same width that stays when they are not
        there, so the round button does not move when they come or go. */}
            <RecordWidget
              trip={widgetTrip}
              {...(() => {
                if (widgetTrip === null) return { elapsed: "", km: "" };
                // No clock time on Record: the trip's screen has it.
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
          </>
        }
      />
    </View>
  );
}
