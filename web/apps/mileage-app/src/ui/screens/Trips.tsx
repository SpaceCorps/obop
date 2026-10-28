import { Banner, Button, Row, Screen, Text } from "@obop/ui-native";
import { useCallback, useEffect, useState } from "react";
import { formatDistance, statusText, tripStatus } from "../../core/present.ts";
import { t } from "../../i18n/index.ts";
import type { Runtime } from "../../native/runtime.ts";
import { useOutbox } from "../hooks.ts";
import type { Nav } from "../router.ts";
import { emitChange } from "../../native/bus.ts";

export function Trips({ rt, nav }: { rt: Runtime; nav: Nav }) {
  const { trips, loaded } = useOutbox(rt);
  const [refreshing, setRefreshing] = useState(false);
  const [offline, setOffline] = useState(false);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await rt.engine.refreshRemote();
      setOffline(false);
      emitChange();
    } catch {
      setOffline(true);
    } finally {
      setRefreshing(false);
    }
  }, [rt]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <Screen
      title={t("trips.title")}
      onBack={nav.back}
      footer={
        <Button
          label={refreshing ? t("trips.refreshing") : t("trips.refresh")}
          variant="secondary"
          busy={refreshing}
          onPress={() => void refresh()}
        />
      }
    >
      {offline ? <Banner tone="info" title={t("trips.offline")} /> : null}
      {loaded && trips.length === 0 ? <Text tone="muted">{t("trips.empty")}</Text> : null}
      {trips.map((trip) => (
        <Row
          key={trip.clientTripId}
          title={new Date(trip.startedAt).toLocaleString()}
          subtitle={statusText(tripStatus(trip))}
          value={formatDistance(trip.distanceM) ?? undefined}
          onPress={() => nav.push({ name: "trip", id: trip.clientTripId })}
          testID={`trip-${trip.clientTripId}`}
        />
      ))}
    </Screen>
  );
}
