import { Banner, Button, Row, Screen, Text, type BannerTone } from "@obop/ui-native";
import { useEffect, useRef, useState } from "react";
import { Switch, View } from "react-native";
import type { TripDetails } from "../../core/details.ts";
import { recordingHealth, type Readiness } from "../../core/liveness.ts";
import { formatKm, phoneTimeZone, totals } from "../../core/present.ts";
import type { TripRepository } from "../../core/repository.ts";
import { LANGUAGE, t, tp, type MessageKey } from "../../i18n/index.ts";
import {
  CannotRecord,
  ensureRecording,
  pauseTrip,
  resumeTrip,
  stopTrip,
} from "../../native/actions.ts";
import { announce } from "../announce.ts";
import { lastEndedTrip, whenText } from "../home.ts";
import {
  useNow,
  useProgress,
  useRegistered,
  useStopDetails,
  useTrips,
  type Check,
} from "../hooks.ts";
import { StopSheet } from "../StopSheet.tsx";
import { tOs } from "../os-text.ts";
import {
  buttonsFor,
  elapsedMs,
  formatElapsed,
  warningOf,
  type Warning,
} from "../recording-view.ts";
import type { Nav } from "../router.ts";

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
 * with Pause at its left, then Resume with Stop at its left while paused. While a trip is open it
 * shows as Current, with its elapsed time and km so far (the same count Stop makes), above the
 * totals of today and this month, which leave the open trip out until it ends. The one warning of
 * a recording that matters most (`warningOf`) has its way on: Restart recording, or the way to the
 * permissions. The totals follow the phone's day.
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
  const now = useNow(active ? 1000 : 30_000);
  const progress = useProgress(store, active?.id ?? null, active?.state ?? null);
  const registered = useRegistered(active?.state === "recording");
  const [restoredRead, setRestoredRead] = useState(restoredNoteRead);
  const [busy, setBusy] = useState<Busy>(null);
  const [actionFailed, setActionFailed] = useState(false);
  const [restartFailed, setRestartFailed] = useState(false);
  const [restartBlocked, setRestartBlocked] = useState<Blocked | null>(null);
  // A check of C5 older than this was answered here (a restart or a resume that worked).
  const [checkHandledAt, setCheckHandledAt] = useState(0);
  // The window of Stop is open, from this button (D4b); `null` while it is closed.
  const [stopSheet, setStopSheet] = useState<Busy>(null);
  const [stopFailed, setStopFailed] = useState(false);
  const [detailsAtStop, setDetailsAtStop] = useStopDetails();

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
  const last = lastEndedTrip(trips);
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
   * Stop (D4b): with the switch on, the window with the details of the trip; off, the trip ends at
   * once, with no window and no confirmation.
   */
  const confirmStop = (which: Busy, id: string) => {
    if (detailsAtStop) {
      setStopFailed(false);
      setStopSheet(which);
      return;
    }
    act(which, async () => {
      await stopTrip(id);
      announce(t("announce.recording.stopped"));
    });
  };

  /** "Stop trip" in the window: the trip ends with the details; a failure keeps the window open. */
  const stopWith = (details: TripDetails) => {
    if (!active || stopSheet === null) return;
    setBusy(stopSheet);
    setStopFailed(false);
    stopTrip(active.id, details)
      .then(
        () => {
          announce(t("announce.recording.stopped"));
          setStopSheet(null);
        },
        () => setStopFailed(true),
      )
      .finally(() => setBusy(null));
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
          <Button
            label={t("home.start")}
            size="big"
            hint={t("home.start.hint")}
            onPress={() => nav.push({ name: "permissions", next: "start" })}
            testID="start"
          />
        );
      case "stop":
        return (
          <Button
            label={t("home.stop")}
            size="big"
            variant="danger"
            hint={t("home.stop.hint")}
            busy={busy === "main"}
            onPress={() => active && confirmStop("main", active.id)}
            testID="stop"
          />
        );
      case "resume":
        return (
          <Button
            label={t("home.resume")}
            size="big"
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
        // The big button is centred on its own; the small one lies over the top left corner of
        // the footer, out of the flow, so it moves nothing when it comes, goes or changes.
        // The switch "Details at Stop" (D4b) lies over the bottom left corner, also out of the flow.
        <View style={{ alignItems: "center", justifyContent: "center" }}>
          {mainButton}
          {sideButton ? (
            <View style={{ position: "absolute", left: 0, top: 0, width: SIDE_WIDTH }}>
              {sideButton}
            </View>
          ) : null}
          <View
            style={{ position: "absolute", left: 0, bottom: 0, width: SIDE_WIDTH, gap: 2 }}
            testID="details-at-stop"
          >
            <Text variant="caption" tone="muted">
              {t("home.details_at_stop")}
            </Text>
            <Switch
              value={detailsAtStop}
              onValueChange={setDetailsAtStop}
              accessibilityLabel={t("home.details_at_stop")}
              testID="details-at-stop-switch"
            />
          </View>
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

      {active ? (
        <View style={{ gap: 4 }} testID="current">
          <Text tone="muted">
            {active.state === "paused"
              ? `${t("home.current")} · ${t("home.paused")}`
              : t("home.current")}
          </Text>
          <Text variant="display" accessibilityRole="timer" testID="elapsed">
            {elapsedText}
          </Text>
          <Text variant="title" testID="current-km">
            {formatKm(progress?.distanceM ?? 0, LANGUAGE)}
          </Text>
          {active.state === "recording" &&
          progress !== null &&
          progress.points === 0 &&
          health.kind === "waiting" ? (
            <Text tone="muted" testID="waiting">
              {t("home.waiting")}
            </Text>
          ) : null}
        </View>
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

      {/* The way to the list of trips (D5), with their number and the last trip under it: its
          km and when, never its name (the list has the rest). */}
      <Row
        title={t("home.trips")}
        subtitle={
          last === null || last.distanceM === null
            ? t("home.none")
            : t("home.last", {
                details: `${formatKm(last.distanceM, LANGUAGE)}, ${when(last.startedAt)}`,
              })
        }
        value={trips.length === 0 ? undefined : tp("trips.count", trips.length)}
        onPress={() => nav.push({ name: "trips" })}
        testID="open-trips"
      />

      <StopSheet
        visible={stopSheet !== null}
        summary={`${elapsedText} · ${formatKm(progress?.distanceM ?? 0, LANGUAGE)}`}
        busy={busy !== null}
        failed={stopFailed}
        onCancel={() => setStopSheet(null)}
        onStop={stopWith}
      />
    </Screen>
  );
}
