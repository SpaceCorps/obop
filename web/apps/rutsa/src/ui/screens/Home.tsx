import { Banner, Button, Screen, Text, type BannerTone } from "@obop/ui-native";
import { useEffect, useRef, useState } from "react";
import { Alert, View, type HostInstance } from "react-native";
import { recordingHealth, type Readiness } from "../../core/liveness.ts";
import { dayKey, formatClock, formatKm, phoneTimeZone, totals } from "../../core/present.ts";
import type { TripRepository } from "../../core/repository.ts";
import { LANGUAGE, t, tp, type MessageKey } from "../../i18n/index.ts";
import {
  CannotRecord,
  ensureRecording,
  pauseTrip,
  resumeTrip,
  stopTrip,
} from "../../native/actions.ts";
import { startWhenReady } from "../../native/start.ts";
import { announce } from "../announce.ts";
import { CurrentWidget } from "../CurrentWidget.tsx";
import { RecentTrips } from "../RecentTrips.tsx";
import { RoundButton } from "../RoundButton.tsx";
import { lastEndedTrip, tripDurationMs, whenText } from "../home.ts";
import { useNow, useProgress, useRegistered, useTrips, type Check } from "../hooks.ts";
import { tOs } from "../os-text.ts";
import {
  buttonsFor,
  elapsedMs,
  formatElapsed,
  warningOf,
  type Warning,
} from "../recording-view.ts";
import type { Nav } from "../router.ts";
import { endedNewestFirst, recentLine } from "../trips-list.ts";

// Read once in this start of the app: the note comes back only with another replaced file.
let restoredNoteRead = false;

type Blocked = Exclude<Readiness, "ready">;

/** Which button's action runs: only that one shows it. */
type Busy = "main" | "side" | "restart" | null;

/** The width of the small button, the same for Pause and Stop, so nothing moves when it changes. */
const SIDE_WIDTH = 92;

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
      return { tone: "danger", title: t("recording.stuck"), action: restart };
    case "no_fix":
      return { tone: "danger", title: tOs("recording.nofix"), action: restart };
    case "quiet":
      return { tone: "warning", title: tp("recording.quiet", warning.minutes), action: restart };
    case "signal":
      return {
        tone: "warning",
        title: t(warning.note.kind === "weak" ? "recording.weak" : "recording.jumpy"),
      };
  }
}

/**
 * The first screen, and the screen of a recording (D4): the big button goes Start, then a red Stop
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
  const { trips, active } = useTrips(store);
  // The content of the footer: the recent trips end above it.
  const footerRef = useRef<HostInstance>(null);
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
  const registered = useRegistered(active?.state === "recording");
  const [restoredRead, setRestoredRead] = useState(restoredNoteRead);
  const [busy, setBusy] = useState<Busy>(null);
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
  const when = (ms: number) => whenText(ms, now, LANGUAGE, zone, words);

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
  const elapsedText = formatElapsed(
    active === null || progress === null
      ? 0
      : elapsedMs({
          // The state the count was read in: right after a Pause or a Resume, until it is read
          // again (at once), the clock goes on as it went, and nothing jumps.
          state: progress.state,
          closedMs: progress.closedMs,
          segmentStartedAt: active.segmentStartedAt,
          now,
        }),
  );

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

  return (
    <Screen
      title={t("app.title")}
      footer={
        // The big button is centred on its own; the small one lies over the top right corner of
        // the footer, out of the flow, so it moves nothing when it comes, goes or changes.
        <View ref={footerRef} style={{ alignItems: "center", justifyContent: "center" }}>
          {mainButton}
          {sideButton ? (
            <View style={{ position: "absolute", right: 0, top: 0, width: SIDE_WIDTH }}>
              {sideButton}
            </View>
          ) : null}
        </View>
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
        elapsed={
          active
            ? elapsedText
            : formatElapsed(
                last === null ? 0 : (tripDurationMs(last, lastProgress?.closedMs ?? null) ?? 0),
              )
        }
        km={formatKm((active ? progress?.distanceM : last?.distanceM) ?? 0, LANGUAGE)}
        started={
          widgetTrip === null
            ? ""
            : // The clock time; with its day when the trip began before today.
              dayKey(widgetTrip.startedAt, zone) === dayKey(now, zone)
              ? formatClock(widgetTrip.startedAt, LANGUAGE, zone)
              : when(widgetTrip.startedAt)
        }
        onPress={() => widgetTrip && nav.push({ name: "trip", id: widgetTrip.id })}
      />

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

      {/* The newest trips that ended, as many as fit above the footer, and the way to History. */}
      <RecentTrips
        lines={endedNewestFirst(trips).map((trip) =>
          recentLine(trip, now, LANGUAGE, zone, {
            today: t("day.today"),
            yesterday: t("day.yesterday"),
            trips: (count) => tp("trips.count", count),
            manual: t("trips.manual"),
          }),
        )}
        footerRef={footerRef}
        onOpen={(id) => nav.push({ name: "trip", id })}
        onHistory={() => nav.push({ name: "trips" })}
      />
    </Screen>
  );
}
