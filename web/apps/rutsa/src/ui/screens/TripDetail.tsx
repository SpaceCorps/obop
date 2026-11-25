import { Banner, Button, Row, Screen, Text, useTheme, useWords } from "@obop/ui-native";
import { useEffect, useRef, useState } from "react";
import { Alert, View } from "react-native";
import { DETAIL_MAX, type TripDetails } from "../../core/details.ts";
import { durationParts, formatKm, phoneTimeZone } from "../../core/present.ts";
import type { TripRepository } from "../../core/repository.ts";
import { LANGUAGE, t, tp } from "../../i18n/index.ts";
import { deleteTrip, updateTripDetails } from "../../native/actions.ts";
import { DetailRow } from "../DetailRow.tsx";
import { InlineText } from "../InlineText.tsx";
import { useFillToFooter } from "../fill.ts";
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
 * as it records, and it has no Delete: it is stopped on the home screen. The name, From, To and
 * Note are changed where they stand (`InlineText`): a tap, typing, and they are kept when they lose
 * the focus; the name is the title, with the default of an untitled trip as its text. In the
 * footer, Back and Delete trip (asked first). The way to the edit of its parts comes with D8.
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
  const words = useWords();
  // The details sent by the last change, until the list is read again (`save`).
  const sent = useRef<TripDetails | null>(null);
  useEffect(() => {
    sent.current = null;
  }, [trip]);
  const [saveFailed, setSaveFailed] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteFailed, setDeleteFailed] = useState(false);

  // The note reaches down to the footer (`useFillToFooter`).
  const note = useFillToFooter();

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
  const name = tripName(trip, (number) =>
    number === null ? t("trip.title") : t("trip.untitled", { number }),
  );
  // The default an untitled trip is called by; the title holds it as text, to be typed over.
  const defaultName = trip.name === null ? name : null;

  /**
   * Keeps one detail typed in place. The others are sent as stored, or as sent a moment ago when
   * the list has not been read again yet, so two changes in a row do not undo each other.
   */
  const save = (field: keyof TripDetails, text: string) => {
    const base = sent.current ?? { name: trip.name, from: trip.from, to: trip.to, note: trip.note };
    // The default left as it is stays no name, so the trip keeps its number.
    const details = { ...base, [field]: field === "name" ? typedName(text, defaultName) : text };
    sent.current = details;
    setSaveFailed(false);
    updateTripDetails(trip.id, details).catch(() => setSaveFailed(true));
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

  /** A row with its label at the left and the detail, changed in place, at the right. */
  const placeRow = (field: "from" | "to") => (
    <DetailRow label={t(field === "from" ? "details.from" : "details.to")}>
      <InlineText
        value={trip[field]}
        placeholder={t("trip.none")}
        label={t(field === "from" ? "details.from" : "details.to")}
        onCommit={(text) => save(field, text)}
        look="value"
        maxLength={DETAIL_MAX[field]}
        testID={`trip-${field}`}
      />
    </DetailRow>
  );

  return (
    <Screen
      title={name}
      // The name is changed where it stands, as the title of the screen.
      titleSlot={
        <InlineText
          value={trip.name ?? defaultName}
          placeholder={name}
          label={t("details.name")}
          onCommit={(text) => save("name", text)}
          look="title"
          maxLength={DETAIL_MAX.name}
          restoreWhenEmpty
          testID="trip-name"
        />
      }
      footer={
        // Back at the left, Delete trip at the right; the open trip has no Delete (it is stopped
        // on the home screen), so Back takes the whole footer.
        <View ref={note.footerRef} style={{ flexDirection: "row", gap: theme.space[3] }}>
          <View style={{ flex: 1 }}>
            <Button
              label={words.back}
              variant="secondary"
              disabled={deleting}
              onPress={nav.back}
              testID="trip-back"
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
      {saveFailed ? (
        <Banner tone="danger" title={t("trip.details.failed")} testID="trip-save-failed" />
      ) : null}

      <View>
        {placeRow("from")}
        {placeRow("to")}
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

      {/* The note under its label, changed in place: it reaches down to the footer, and a longer
          note makes it grow and the screen scroll. */}
      <View
        ref={note.blockRef}
        onLayout={note.onLayout}
        style={{
          minHeight: note.minHeight,
          gap: theme.space[1],
          paddingVertical: theme.space[3],
          paddingHorizontal: theme.space[4],
          backgroundColor: theme.c.surface,
          borderBottomWidth: 1,
          borderBottomColor: theme.c.border,
        }}
      >
        <Text variant="label">{t("details.note")}</Text>
        <InlineText
          value={trip.note}
          placeholder={t("trip.note.empty")}
          label={t("details.note")}
          onCommit={(text) => save("note", text)}
          look="note"
          maxLength={DETAIL_MAX.note}
          testID="trip-note"
        />
      </View>
    </Screen>
  );
}
