import { Banner, Button, Screen, Text } from "@obop/ui-native";
import { useState } from "react";
import { View } from "react-native";
import { formatKm, phoneTimeZone, totals } from "../../core/present.ts";
import type { TripRepository } from "../../core/repository.ts";
import { LANGUAGE, t } from "../../i18n/index.ts";
import { lastEndedTrip, whenText } from "../home.ts";
import { useNow, useTrips } from "../hooks.ts";
import type { Nav } from "../router.ts";

// Read once in this start of the app: the note comes back only with another replaced file.
let restoredNoteRead = false;

/**
 * The first screen: Start, the kilometres of today and of this month, the last trip, and the
 * notes that matter now (a trip that is open; a file of a backup that no key could open, replaced
 * at this start). The totals follow the phone's day: they are counted again when the trips change
 * and every 30 s, so that after midnight "today" starts at 0.
 *
 * After `Home` of `mileage-app` (`src/ui/screens/Home.tsx`), without the purpose of stage 2 and
 * what talks to its server.
 */
export function Home({
  store,
  nav,
  recoveredStaleFile,
}: {
  store: TripRepository;
  nav: Nav;
  recoveredStaleFile: boolean;
}) {
  const { trips, active } = useTrips(store);
  const now = useNow(30_000);
  const [restoredRead, setRestoredRead] = useState(restoredNoteRead);
  const zone = phoneTimeZone();
  const sum = totals(trips, now, zone);
  const last = lastEndedTrip(trips);
  const words = { today: t("day.today"), yesterday: t("day.yesterday") };
  const when = (ms: number) => whenText(ms, now, LANGUAGE, zone, words);

  return (
    <Screen title={t("app.title")}>
      {active ? (
        <Banner
          tone="info"
          title={t(active.state === "paused" ? "home.open.paused" : "home.open.recording")}
          message={t("home.open.message", { when: when(active.startedAt) })}
          action={{ label: t("home.open.action"), onPress: () => nav.push({ name: "recording" }) }}
          testID="open-trip"
        />
      ) : null}
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

      <View style={{ paddingVertical: 24 }}>
        <Button
          label={t("home.start")}
          size="big"
          hint={t("home.start.hint")}
          disabled={active !== null}
          onPress={() => nav.push({ name: "permissions", next: "start" })}
          testID="start"
        />
      </View>

      <Text tone="muted" testID="last-trip">
        {last === null || last.distanceM === null
          ? t("home.none")
          : t("home.last", {
              distance: formatKm(last.distanceM, LANGUAGE),
              when: when(last.startedAt),
            })}
      </Text>
    </Screen>
  );
}
