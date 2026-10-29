import { Banner, Button, Row, Screen, Text } from "@obop/ui-native";
import { useState } from "react";
import { View } from "react-native";
import { formatDistance, statusText, tripStatus } from "../../core/present.ts";
import { hasWork } from "../../core/store.ts";
import { PURPOSES, type Purpose } from "../../core/types.ts";
import { formatDateTime, t, tp, type MessageKey } from "../../i18n/index.ts";
import type { Runtime } from "../../native/runtime.ts";
import { useOutbox, useStaleFileNote, useSyncStatus } from "../hooks.ts";
import type { Nav } from "../router.ts";

export function Home({ rt, nav }: { rt: Runtime; nav: Nav }) {
  const { trips, active } = useOutbox(rt);
  const sync = useSyncStatus(rt);
  const restored = useStaleFileNote(rt);
  const [purpose, setPurpose] = useState<Purpose>("business");

  const last = trips.find((trip) => trip.clientTripId !== active?.clientTripId);
  const waiting = trips.filter(
    (trip) => trip.origin === "device" && hasWork(trip) && trip !== active,
  );
  const problem = trips.some((trip) => tripStatus(trip) === "problem");

  return (
    <Screen
      title={t("home.title")}
      footer={
        <>
          <Row
            title={t("home.trips")}
            onPress={() => nav.push({ name: "trips" })}
            testID="open-trips"
          />
          <Row
            title={t("home.account")}
            onPress={() => nav.push({ name: "account" })}
            testID="open-account"
          />
        </>
      }
    >
      {active ? (
        <Banner
          tone="warning"
          title={t("home.open.title")}
          message={t("home.open.message", { when: formatDateTime(active.startedAt) })}
          action={{ label: t("home.open.action"), onPress: () => nav.push({ name: "recording" }) }}
          testID="open-trip-banner"
        />
      ) : null}
      {restored.recoveredAt !== null ? (
        <Banner
          tone="warning"
          title={t("home.restored.title")}
          message={t("home.restored.message")}
          action={{ label: t("home.restored.action"), onPress: restored.dismiss }}
          testID="restored-banner"
        />
      ) : null}
      {problem ? (
        <Banner
          tone="danger"
          title={t("home.problem.title")}
          message={t("home.problem.message")}
          action={{ label: t("home.problem.action"), onPress: () => nav.push({ name: "trips" }) }}
        />
      ) : null}
      {waiting.length > 0 && !problem ? (
        <Banner
          tone="info"
          title={t("home.waiting.title")}
          message={tp("home.waiting.message", waiting.length)}
        />
      ) : null}
      {sync.error ? <Banner tone="danger" title={t("error.sync")} testID="sync-error" /> : null}

      <Text variant="label">{t("home.purpose")}</Text>
      <View style={{ flexDirection: "row", gap: 8, flexWrap: "wrap" }}>
        {PURPOSES.map((p) => (
          <Button
            key={p}
            label={t(`purpose.${p}` as MessageKey)}
            variant={p === purpose ? "primary" : "secondary"}
            selected={p === purpose}
            onPress={() => setPurpose(p)}
            testID={`purpose-${p}`}
          />
        ))}
      </View>

      <View style={{ paddingVertical: 24 }}>
        <Button
          label={t("home.start")}
          size="big"
          hint={t("home.start.hint")}
          disabled={active !== null}
          onPress={() => nav.push({ name: "permissions", purpose })}
          testID="start"
        />
      </View>

      {last ? (
        <Text tone="muted">
          {t("home.last", {
            distance: formatDistance(last.distanceM) ?? t("common.unknown"),
            state: statusText(tripStatus(last)),
          })}
        </Text>
      ) : (
        <Text tone="muted">{t("home.none")}</Text>
      )}
    </Screen>
  );
}
