import { Banner, Screen, Text, useTheme, type BannerTone } from "@obop/ui-native";
import { useState } from "react";
import { View } from "react-native";
import { formatKm, phoneTimeZone, totals } from "../../core/present.ts";
import type { TripRepository } from "../../core/repository.ts";
import { LANGUAGE, t, tp, type MessageKey } from "../../i18n/index.ts";
import { useNow, useTrips } from "../hooks.ts";
import { tOs } from "../os-text.ts";
import type { Blocked } from "../RecordPanel.tsx";
import type { Warning } from "../recording-view.ts";
import { RecentTrips } from "../RecentTrips.tsx";
import type { Nav } from "../router.ts";
import { GROUP_INSET, GROUP_RADIUS } from "../Group.tsx";
import { HEADER_BUTTON, ScreenHeader } from "../ScreenHeader.tsx";
import { SectionHeader } from "../SectionHeader.tsx";
import { recentLine, recentTrips } from "../trips-list.ts";

// Read once in this start of the app: the note comes back only with another replaced file.
let restoredNoteRead = false;

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
 * The first screen, the tab Record (D4): the one warning of a recording that matters most
 * (`warningOf`, from `useRecordPanel`) with its way on (Restart recording, or the way to the
 * permissions), the totals of today and this month (they leave the open trip out until it ends;
 * they follow the phone's day) and the trips of today and yesterday that ended. The trip being recorded and its
 * buttons are in the switcher at the bottom, the same on every tab (`RecordPanel.tsx`).
 */
export function Home({
  store,
  nav,
  recoveredStaleFile,
  warning,
  restart,
}: {
  store: TripRepository;
  nav: Nav;
  recoveredStaleFile: boolean;
  /** The warning of the recording, from `useRecordPanel`. */
  warning: Warning | null;
  /** Restart recording, from `useRecordPanel`. */
  restart: () => void;
}) {
  const theme = useTheme();
  const { trips, active } = useTrips(store);
  const now = useNow(active ? 1000 : 30_000);
  const [restoredRead, setRestoredRead] = useState(restoredNoteRead);
  const zone = phoneTimeZone();
  const sum = totals(trips, now, zone);

  const shown =
    warning === null
      ? null
      : warningBanner(warning, {
          restart,
          fix: () => nav.push({ name: "permissions", next: "restart" }),
        });

  return (
    <Screen
      floatingHeaderHeight={HEADER_BUTTON}
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

      {/* The totals under a header on a card, as Recent trips. */}
      <View style={{ gap: theme.space[2] }} testID="statistics">
        <SectionHeader title={t("home.statistics")} />
        <View
          style={{
            flexDirection: "row",
            gap: 24,
            paddingHorizontal: GROUP_INSET,
            paddingVertical: theme.space[3],
            borderRadius: GROUP_RADIUS,
            backgroundColor: theme.c.surface,
          }}
        >
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

      {/* All the trips of today and yesterday, and the way to History. */}
      <RecentTrips
        lines={recentTrips(trips, now, zone).map((trip) =>
          recentLine(trip, now, LANGUAGE, zone, {
            today: t("day.today"),
            yesterday: t("day.yesterday"),
            trips: (count) => tp("trips.count", count),
            manual: t("trips.manual"),
          }),
        )}
        onOpen={(id) => nav.push({ name: "trip", id })}
        onHistory={() => nav.tab("history")}
      />
    </Screen>
  );
}
