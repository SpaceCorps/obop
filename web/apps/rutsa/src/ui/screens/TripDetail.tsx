import { Banner, Screen, Text, useTheme, useWords } from "@obop/ui-native";
import { useEffect, useRef, useState } from "react";
import { ActivityIndicator, Alert, Animated, Keyboard, Pressable, View } from "react-native";
import { DETAIL_MAX, type TripDetails } from "../../core/details.ts";
import { phoneTimeZone } from "../../core/present.ts";
import type { TripRepository } from "../../core/repository.ts";
import { LANGUAGE, t, tp } from "../../i18n/index.ts";
import { deleteTrip, updateTripDetails } from "../../native/actions.ts";
import { CurrentWidget } from "../CurrentWidget.tsx";
import { DetailRow } from "../DetailRow.tsx";
import { Group, GROUP_INSET, GROUP_RADIUS } from "../Group.tsx";
import { DELETE_RED } from "../RoundButton.tsx";
import { Icon } from "../icons.tsx";
import { InlineText } from "../InlineText.tsx";
import { HEADER_BUTTON, ScreenHeader } from "../ScreenHeader.tsx";
import { TripNameInput } from "../TripNameInput.tsx";
import { useSquare } from "../square.ts";
import { useNow, useProgress, useTrips } from "../hooks.ts";
import type { Nav } from "../router.ts";
import { tripName, typedName } from "../trip-label.ts";
import { tripDate, tripTimes } from "../trip-view.ts";
import { widgetValues } from "../widget-values.ts";

/**
 * One trip (D6), the open one too. On top the current widget of the home screen, in the same place
 * (under the same header): its name, typed in place, its state, its time without the pauses, its
 * km and when it started or ended (with its day when not today). Under it From and To ("Not
 * given" when empty), then its date and times and how many positions a recorded trip has and how many the
 * distance left out (the kind of a trip is marked in the list, BR-28), then its note (an empty
 * block when there is none), at least as high as it is wide. While the trip is open its time, km and
 * positions grow as it records. Back at the left of the header, Edit at the right: the screen is
 * read only until Edit is pressed; then the name, From, To and Note are typed where they stand
 * (`InlineText`), Back becomes Cancel ("x": what was typed is dropped) and Edit becomes Done
 * ("✓": what was typed is kept, all at once), and a red Delete trip (asked first) comes under
 * the note; the open trip has none: it is stopped on the home screen. The edit of its parts
 * (times, km) is in stage 2.
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
  const [saveFailed, setSaveFailed] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteFailed, setDeleteFailed] = useState(false);
  // Read only until Edit is pressed.
  const [editing, setEditing] = useState(false);
  // What was typed while editing, kept by Done and dropped by Cancel.
  const [draft, setDraft] = useState<Partial<Record<keyof TripDetails, string>>>({});
  // Changed by Cancel, so the fields are drawn again with what is stored.
  const [session, setSession] = useState(0);
  const deletable = editing && !open;

  // The note is at least as high as it is wide (`useSquare`); Delete trip comes under it.
  const note = useSquare();
  // The details come in softly under the card of the trip, which does not move.
  const fade = useRef(new Animated.Value(0)).current;
  useEffect(() => {
    Animated.timing(fade, { toValue: 1, duration: 200, useNativeDriver: true }).start();
  }, [fade]);

  if (trip === null) {
    return (
      <Screen
        floatingHeaderHeight={HEADER_BUTTON}
        title={t("trip.title")}
        titleSlot={
          <ScreenHeader
            title={t("trip.title")}
            left={{ icon: "back", label: words.back, onPress: nav.back, testID: "trip-back" }}
          />
        }
      >
        {loaded ? (
          <Text tone="muted" testID="trip-notfound">
            {t("trip.notfound")}
          </Text>
        ) : null}
      </Screen>
    );
  }

  const zone = phoneTimeZone();
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

  /** One detail as it is typed, kept in the draft until Done. */
  const type = (field: keyof TripDetails) => (text: string) =>
    setDraft((d) => ({ ...d, [field]: text }));

  /** Done: keeps what was typed, all of it at once, and the screen is read only again. */
  const done = () => {
    Keyboard.dismiss();
    setEditing(false);
    if (Object.keys(draft).length > 0) {
      const details: TripDetails = {
        // The default left as it is stays no name, so the trip keeps its number.
        name: draft.name === undefined ? trip.name : typedName(draft.name, defaultName),
        from: draft.from ?? trip.from,
        to: draft.to ?? trip.to,
        note: draft.note ?? trip.note,
      };
      setSaveFailed(false);
      updateTripDetails(trip.id, details).catch(() => setSaveFailed(true));
    }
    setDraft({});
  };

  /** Cancel: what was typed is dropped, the fields show what is stored, read only again. */
  const cancel = () => {
    Keyboard.dismiss();
    setEditing(false);
    setDraft({});
    setSession((s) => s + 1);
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
        key={session}
        value={trip[field]}
        placeholder={t("trip.none")}
        label={t(field === "from" ? "details.from" : "details.to")}
        onChange={type(field)}
        look="value"
        maxLength={DETAIL_MAX[field]}
        editable={editing}
        testID={`trip-${field}`}
      />
    </DetailRow>
  );

  return (
    <Screen
      floatingHeaderHeight={HEADER_BUTTON}
      title={name}
      // A header as high as the home screen's, so the current widget stands where it stood there
      // when it was tapped. Back at the left and the pill Edit at the right; while editing, Cancel
      // ("x") at the left and Done ("✓", filled) at the right.
      titleSlot={
        <ScreenHeader
          title={t("trip.title")}
          left={
            editing
              ? {
                  icon: "close",
                  label: t("common.cancel"),
                  onPress: cancel,
                  disabled: deleting,
                  testID: "trip-cancel",
                }
              : {
                  icon: "back",
                  label: words.back,
                  onPress: nav.back,
                  disabled: deleting,
                  testID: "trip-back",
                }
          }
          right={
            editing
              ? {
                  icon: "check",
                  label: t("header.done"),
                  look: "filled",
                  onPress: done,
                  disabled: deleting,
                  testID: "trip-done",
                }
              : {
                  text: t("header.edit"),
                  label: t("header.edit"),
                  onPress: () => setEditing(true),
                  testID: "trip-edit",
                }
          }
        />
      }
    >
      {/* The card of the trip, as on the home screen, with its name typed in place. */}
      <CurrentWidget
        trip={trip}
        {...widgetValues(trip, progress, now, LANGUAGE, zone)}
        nameSlot={
          <TripNameInput
            key={session}
            trip={trip}
            onChange={type("name")}
            editable={editing}
            // Read only, "Untitled trip N" is the trip's name (black); while editing it is an
            // empty field (grey), as on Manual add.
            placeholderTone={editing ? "muted" : "text"}
          />
        }
      />

      {/* The rest comes in under the card, which stays in place. */}
      <Animated.View style={{ opacity: fade, gap: theme.space[4] }}>
        {deleteFailed ? (
          <Banner tone="danger" title={t("trip.delete.failed")} testID="trip-delete-failed" />
        ) : null}
        {saveFailed ? (
          <Banner tone="danger" title={t("trip.details.failed")} testID="trip-save-failed" />
        ) : null}

        <Group>
          {placeRow("from")}
          {placeRow("to")}
        </Group>

        {/* Its date, as on Manual add (read only: its edit is in stage 2), then its times. */}
        <Group>
          <DetailRow label={t("trip.date")} testID="trip-date">
            <Text tone="muted">{tripDate(trip, Date.now(), LANGUAGE, zone)}</Text>
          </DetailRow>
          <DetailRow label={t("trip.time")} testID="trip-time">
            <Text tone="muted">
              {open
                ? t("trip.time.open", { start: tripTimes(trip, LANGUAGE, zone) })
                : tripTimes(trip, LANGUAGE, zone)}
            </Text>
          </DetailRow>
          {trip.source === "recorded" ? (
            <DetailRow label={t("trip.positions")} testID="trip-positions">
              <Text tone="muted">{positions ?? ""}</Text>
            </DetailRow>
          ) : null}
        </Group>

        {/* The note under its label, with no placeholder: an empty note is an empty block, and a
          tap anywhere in it while editing puts the cursor there. It is at least as high as it is
          wide, and a longer note makes it grow and the screen scroll. */}
        <View
          onLayout={note.onLayout}
          style={{
            minHeight: note.minHeight,
            gap: theme.space[1],
            paddingVertical: theme.space[3],
            paddingHorizontal: GROUP_INSET,
            backgroundColor: theme.c.surface,
            borderRadius: GROUP_RADIUS,
          }}
        >
          <Text variant="label">{t("details.note")}</Text>
          <InlineText
            key={session}
            value={trip.note}
            placeholder=""
            label={t("details.note")}
            onChange={type("note")}
            look="note"
            maxLength={DETAIL_MAX.note}
            editable={editing}
            testID="trip-note"
          />
        </View>

        {/* While editing a trip that ended: Delete trip, red, on the background, asked first. */}
        {deletable ? (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={t("trip.delete")}
            accessibilityState={{ busy: deleting }}
            disabled={deleting}
            onPress={confirmDelete}
            hitSlop={8}
            testID="trip-delete"
            style={({ pressed }) => ({
              flexDirection: "row",
              alignItems: "center",
              alignSelf: "flex-start",
              gap: theme.space[3],
              paddingHorizontal: GROUP_INSET,
              paddingVertical: theme.space[2],
              opacity: pressed ? 0.6 : 1,
            })}
          >
            {/* The red of recording, a little softer: thin words on the dark ground look redder
              than the square of Stop on its grey disc. */}
            {deleting ? (
              <ActivityIndicator size="small" color={DELETE_RED} />
            ) : (
              <Icon name="trash" color={DELETE_RED} size={20} />
            )}
            <Text variant="label" color={DELETE_RED}>
              {t("trip.delete")}
            </Text>
          </Pressable>
        ) : null}
      </Animated.View>
    </Screen>
  );
}
