import { Banner, Button, Row, Screen, Text, useTheme } from "@obop/ui-native";
import { useState } from "react";
import { Alert, View } from "react-native";
import type { TripDetails } from "../../core/details.ts";
import { durationParts, formatKm, phoneTimeZone } from "../../core/present.ts";
import type { TripRepository } from "../../core/repository.ts";
import { LANGUAGE, t, tp } from "../../i18n/index.ts";
import { deleteTrip, updateTripDetails } from "../../native/actions.ts";
import { DetailsSheet } from "../DetailsSheet.tsx";
import { tripDurationMs } from "../home.ts";
import { useNow, useProgress, useTrips } from "../hooks.ts";
import { elapsedMs, formatElapsed } from "../recording-view.ts";
import type { Nav } from "../router.ts";
import { tripName, typedName } from "../trip-label.ts";
import { tripDate, tripTimes } from "../trip-view.ts";

/** A duration in the words of the catalogue: `12 min`, `1 h 5 min`. */
function durationText(ms: number): string {
  const { hours, minutes } = durationParts(0, ms);
  return hours === 0
    ? t("duration.minutes", { minutes })
    : t("duration.hoursMinutes", { hours, minutes });
}

/**
 * One trip (D6), the open one too. Its name as the title ("Untitled trip 12" while it has none),
 * then From and To ("Not given" when empty); its date and times, how long it took without the
 * pauses, its km, how many positions a recorded trip has and how many the distance left out (the
 * kind of a trip is marked in the list, BR-28); then its note ("Empty"), as long as it is. While
 * the trip is open (from the current widget of the home screen) its time, km and positions grow
 * as it records, and it has no Delete: it is stopped on the home screen. In the footer, Edit
 * details (the window with the details as stored, the default name of an untitled trip in Name,
 * and Save) and Delete trip (asked first). The way to the edit of its parts comes with D8.
 *
 * After `TripDetail` of `mileage-app` (`src/ui/screens/TripDetail.tsx`), without its server: no
 * purpose (stage 2), no submit, and Delete instead of Discard with a reason.
 */
export function TripDetail({ store, nav, id }: { store: TripRepository; nav: Nav; id: string }) {
  const theme = useTheme();
  const { trips, loaded } = useTrips(store);
  const trip = trips.find((x) => x.id === id) ?? null;
  const open = trip !== null && trip.state !== "ended";
  // The count of a recorded trip: read again as positions are stored, so it grows while it records.
  const progress = useProgress(
    store,
    trip?.source === "recorded" ? trip.id : null,
    trip?.state ?? null,
  );
  const now = useNow(open ? 1000 : 60_000);
  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveFailed, setSaveFailed] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteFailed, setDeleteFailed] = useState(false);

  if (trip === null) {
    return (
      <Screen title={t("trip.title")} onBack={nav.back}>
        {loaded ? (
          <Text tone="muted" testID="trip-notfound">
            {t("trip.notfound")}
          </Text>
        ) : null}
      </Screen>
    );
  }

  const zone = phoneTimeZone();
  // Open: the clock of the current widget; ended: its time in minutes.
  const duration = open
    ? progress === null
      ? null
      : formatElapsed(
          elapsedMs({
            state: progress.state,
            closedMs: progress.closedMs,
            segmentStartedAt: trip.segmentStartedAt,
            now,
          }),
        )
    : (() => {
        const ms = tripDurationMs(trip, progress?.closedMs ?? null);
        return ms === null ? null : durationText(ms);
      })();
  const distanceM = open ? (progress?.distanceM ?? 0) : (trip.distanceM ?? 0);
  const positions =
    progress === null
      ? null
      : progress.dropped === 0
        ? tp("trip.positions.count", progress.points)
        : t("trip.positions.dropped", {
            positions: tp("trip.positions.count", progress.points),
            dropped: progress.dropped,
          });
  const given = (text: string | null) => text ?? t("trip.none");
  const name = tripName(trip, (number) =>
    number === null ? t("trip.title") : t("trip.untitled", { number }),
  );
  // The default an untitled trip is called by; the window opens with it in the field Name.
  const defaultName = trip.name === null ? name : null;

  const save = (details: TripDetails) => {
    setSaving(true);
    setSaveFailed(false);
    // The default left as it is stays no name, so the trip keeps its number.
    updateTripDetails(trip.id, { ...details, name: typedName(details.name, defaultName) })
      .then(
        () => setEditing(false),
        () => setSaveFailed(true),
      )
      .finally(() => setSaving(false));
  };

  const confirmDelete = () => {
    Alert.alert(t("trip.delete.title"), t("trip.delete.message"), [
      { text: t("common.cancel"), style: "cancel" },
      {
        text: t("common.delete"),
        style: "destructive",
        onPress: () => {
          setDeleting(true);
          setDeleteFailed(false);
          deleteTrip(trip.id).then(
            () => nav.back(),
            () => {
              setDeleting(false);
              setDeleteFailed(true);
            },
          );
        },
      },
    ]);
  };

  return (
    <Screen
      title={name}
      onBack={nav.back}
      footer={
        // Two halves: Edit details at the left, Delete trip at the right; the open trip has no
        // Delete (it is stopped on the home screen).
        <View style={{ flexDirection: "row", gap: theme.space[3] }}>
          <View style={{ flex: 1 }}>
            <Button
              label={t("trip.details.edit")}
              variant="secondary"
              disabled={deleting}
              onPress={() => {
                setSaveFailed(false);
                setEditing(true);
              }}
              testID="trip-edit-details"
            />
          </View>
          {open ? null : (
            <View style={{ flex: 1 }}>
              <Button
                label={t("trip.delete")}
                variant="danger"
                busy={deleting}
                onPress={confirmDelete}
                testID="trip-delete"
              />
            </View>
          )}
        </View>
      }
    >
      {deleteFailed ? (
        <Banner tone="danger" title={t("trip.delete.failed")} testID="trip-delete-failed" />
      ) : null}

      <View>
        <Row title={t("details.from")} value={given(trip.from)} testID="trip-from" />
        <Row title={t("details.to")} value={given(trip.to)} testID="trip-to" />
      </View>

      <View>
        <Row
          title={t("trip.date")}
          value={tripDate(trip, Date.now(), LANGUAGE, zone)}
          testID="trip-date"
        />
        <Row
          title={t("trip.time")}
          value={
            open
              ? t("trip.time.open", { start: tripTimes(trip, LANGUAGE, zone) })
              : tripTimes(trip, LANGUAGE, zone)
          }
          testID="trip-time"
        />
        <Row title={t("trip.duration")} value={duration ?? undefined} testID="trip-duration" />
        <Row
          title={t("trip.distance")}
          value={formatKm(distanceM, LANGUAGE)}
          testID="trip-distance"
        />
        {trip.source === "recorded" ? (
          <Row title={t("trip.positions")} value={positions ?? undefined} testID="trip-positions" />
        ) : null}
      </View>

      {/* The note under its label, as long as it is: it wraps and the screen scrolls. */}
      <Row
        title={t("details.note")}
        subtitle={trip.note ?? t("trip.note.empty")}
        testID="trip-note"
      />

      <DetailsSheet
        visible={editing}
        title={t("trip.details.title")}
        initial={{ name: trip.name ?? defaultName, from: trip.from, to: trip.to, note: trip.note }}
        confirmLabel={t("common.save")}
        busy={saving}
        failedText={saveFailed ? t("trip.details.failed") : null}
        onCancel={() => setEditing(false)}
        onConfirm={save}
        testID="trip-details-sheet"
      />
    </Screen>
  );
}
