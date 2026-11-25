import { Banner, Screen, Text, useTheme, useWords } from "@obop/ui-native";
import { useState, type ReactNode } from "react";
import { Alert, Pressable, Text as PlainText, View } from "react-native";
import { DETAIL_MAX } from "../../core/details.ts";
import {
  FAST_KMH,
  MAX_KM,
  checkManual,
  durationFromParts,
  parseDuration,
  type ManualField,
  type ManualForm,
  type ManualProblem,
} from "../../core/manual.ts";
import { dayKey, formatClock, formatDay, momentOf, phoneTimeZone } from "../../core/present.ts";
import type { TripRepository } from "../../core/repository.ts";
import { LANGUAGE, t } from "../../i18n/index.ts";
import { addManualTrip } from "../../native/actions.ts";
import { CurrentWidget } from "../CurrentWidget.tsx";
import { DetailRow } from "../DetailRow.tsx";
import { useFillToBottom } from "../fill.ts";
import { useTrips } from "../hooks.ts";
import { InlineText } from "../InlineText.tsx";
import { ScreenHeader } from "../ScreenHeader.tsx";
import { IosPicker, pickerInRow, pickOnAndroid, type PickMode } from "../pick.tsx";
import { formatElapsed } from "../recording-view.ts";
import { tripTimes } from "../trip-view.ts";
import type { Nav } from "../router.ts";

/** The words of a problem under its field. */
function problemText(problem: ManualProblem, field: ManualField): string {
  switch (problem) {
    case "km_too_far":
      return t("manual.problem.km_too_far", {
        max: new Intl.NumberFormat(LANGUAGE).format(MAX_KM),
      });
    case "too_long":
      // Only the texts can be too long: the name, From, To and the note.
      return t("manual.problem.too_long", {
        max: field in DETAIL_MAX ? DETAIL_MAX[field as keyof typeof DETAIL_MAX] : 0,
      });
    default:
      return t(`manual.problem.${problem}`);
  }
}

/** `08:30` as hours and minutes; `null` when it is not one. */
function clockOf(text: string): { hour: number; minute: number } | null {
  const match = /^(\d{1,2}):(\d{2})$/.exec(text);
  return match ? { hour: Number(match[1]), minute: Number(match[2]) } : null;
}

/**
 * A trip added by hand (D7, BR-28), in the look of a trip's screen. Under the header "Manual add"
 * the current widget of the trip being typed, state New: its name and its km typed in place, the
 * big time is how long it took, its hours, minutes and seconds typed in place (each empty part is
 * 0, shown as a grey "00"), and the time at the bottom right is when it started, chosen with the
 * clock of the system on a tap. The start is at first the moment the screen was opened. Under it From and To typed in place; then Date (a picker) and
 * Time, the start and the end that follows from the duration (`08:30–09:15`, an end on
 * another day with that day in brackets, as on a trip's screen); then Note, down to the bottom; Back and Save are the round buttons
 * of the header. Save
 * checks it all (`checkManual`): each problem is said under its place, and an average speed over
 * `FAST_KMH` asks first. The trip is stored as manual and the screen goes back. Reachable without
 * location (US-18 AC1).
 */
export function AddTrip({ store, nav }: { store: TripRepository; nav: Nav }) {
  const theme = useTheme();
  const shared = useWords();
  const { trips } = useTrips(store);
  const note = useFillToBottom();
  const zone = phoneTimeZone();
  const [form, setForm] = useState<ManualForm>(() => {
    // The moment Add trip was pressed: the start until another is chosen.
    const opened = Date.now();
    return {
      name: "",
      date: dayKey(opened, zone),
      start: formatClock(opened, "en-GB", zone),
      // Made of `parts` below.
      duration: "",
      km: "",
      from: "",
      to: "",
      note: "",
    };
  });
  // Problems are said once Save was pressed, and from then on as the driver types.
  const [tried, setTried] = useState(false);
  const [saving, setSaving] = useState(false);
  const [failed, setFailed] = useState(false);
  // The hours, minutes and seconds of the duration as typed; an empty part is 0.
  const [parts, setParts] = useState({ hours: "", minutes: "", seconds: "" });
  const setPart = (part: keyof typeof parts) => (text: string) =>
    setParts((p) => ({ ...p, [part]: text }));

  const now = Date.now();
  const duration = durationFromParts(parts.hours, parts.minutes, parts.seconds);
  const result = checkManual({ ...form, duration }, now, zone, trips);
  const set = (field: ManualField) => (text: string) => setForm((f) => ({ ...f, [field]: text }));
  const problem = (field: ManualField): string | undefined => {
    const found = result.problems[field];
    return tried && found ? problemText(found, field) : undefined;
  };

  const start = clockOf(form.start) ?? { hour: 12, minute: 0 };
  const seconds = parseDuration(duration) ?? 0;
  /** What the pickers open on, as the phone's clock reads them. */
  const values = {
    date: new Date(momentOf(form.date, 12, 0, zone)),
    start: new Date(momentOf(form.date, start.hour, start.minute, zone)),
  };
  const chosen = {
    date: (date: Date) => set("date")(dayKey(date.getTime(), zone)),
    start: (date: Date) => set("start")(formatClock(date.getTime(), "en-GB", zone)),
  };

  /** A text that opens a picker on a tap. */
  const tappable = (label: string, shown: ReactNode, testID: string, onPress: () => void) => (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      hitSlop={8}
      onPress={onPress}
      testID={testID}
      style={({ pressed }) => ({ opacity: pressed ? 0.6 : 1 })}
    >
      {shown}
    </Pressable>
  );

  /**
   * The start, chosen with the clock of the system: on Android its dialog opens on a tap, on iOS
   * the compact picker stands in its place.
   */
  const startPicker = (shown: ReactNode): ReactNode =>
    pickerInRow ? (
      <IosPicker mode="time" value={values.start} onPick={chosen.start} testID="manual-start" />
    ) : (
      tappable(t("manual.start"), shown, "manual-start", () => {
        void pickOnAndroid("time", values.start).then((date) => {
          if (date) chosen.start(date);
        });
      })
    );

  /** The row Date: the dialog of Android on a tap, the picker of iOS in it. */
  const dateRow = () => {
    const mode: PickMode = "date";
    const max = new Date(now);
    const otherYear = form.date.slice(0, 4) !== dayKey(now, zone).slice(0, 4);
    return (
      <DetailRow
        label={t("manual.date")}
        error={problem("date")}
        {...(pickerInRow
          ? {}
          : {
              onPress: () => {
                void pickOnAndroid(mode, values.date, max).then((date) => {
                  if (date) chosen.date(date);
                });
              },
            })}
        testID="manual-date"
      >
        {pickerInRow ? (
          <IosPicker mode={mode} value={values.date} maximumDate={max} onPick={chosen.date} />
        ) : (
          // Black: the date is given from the start (today), it is not an empty field.
          <Text>{formatDay(values.date.getTime(), LANGUAGE, zone, otherYear)}</Text>
        )}
      </DetailRow>
    );
  };

  /** A row whose value is typed in place. */
  const typeRow = (field: "from" | "to", label: string) => (
    <DetailRow label={label} error={problem(field)} testID={`manual-${field}`}>
      <InlineText
        value={form[field] === "" ? null : form[field]}
        placeholder={t("trip.none")}
        label={label}
        onChange={set(field)}
        look="value"
        maxLength={DETAIL_MAX[field]}
        testID={`manual-${field}-input`}
      />
    </DetailRow>
  );

  const save = () => {
    setTried(true);
    const trip = result.trip;
    if (trip === null) return;
    const keep = () => {
      setSaving(true);
      setFailed(false);
      addManualTrip(trip).then(
        () => nav.back(),
        () => {
          setSaving(false);
          setFailed(true);
        },
      );
    };
    if (result.speedKmh !== null && result.speedKmh > FAST_KMH) {
      Alert.alert(
        t("manual.fast.title"),
        t("manual.fast.message", { speed: Math.round(result.speedKmh) }),
        [
          { text: t("common.cancel"), style: "cancel" },
          { text: t("manual.fast.save"), onPress: keep },
        ],
      );
      return;
    }
    keep();
  };

  // The number the trip gets when it is saved without a name, as the store gives it.
  const number = Math.max(0, ...trips.map((trip) => trip.number ?? 0)) + 1;
  const elapsed = formatElapsed(seconds * 1000);
  const times =
    result.times === null
      ? form.start
      : // As on a trip's screen: an end on another day says that day in brackets.
        tripTimes(result.times, LANGUAGE, zone);
  // A duration that cannot be (minutes over 59) or none at all is said under the widget, where it
  // is typed; an end after now under Time.
  const durationProblem =
    result.problems.duration === "end_future" ? undefined : problem("duration");
  const widgetProblem = problem("km") ?? durationProblem ?? problem("name");

  return (
    <Screen
      title={t("manual.title")}
      // Back at the left (nothing is saved), Save at the right, filled: the action of the screen.
      titleSlot={
        <ScreenHeader
          title={t("manual.title")}
          left={{
            icon: "back",
            label: shared.back,
            onPress: nav.back,
            disabled: saving,
            testID: "manual-back",
          }}
          right={{
            icon: "check",
            label: t("common.save"),
            onPress: save,
            look: "filled",
            busy: saving,
            testID: "manual-save",
          }}
        />
      }
    >
      {failed ? <Banner tone="danger" title={t("manual.failed")} testID="manual-failed" /> : null}

      {/* The card of the trip, as on a trip's screen: the name and the km typed in place, the
        duration and the start chosen with a tap. */}
      <View style={{ gap: theme.space[1] }}>
        <CurrentWidget
          trip={{ state: "new", name: null, number }}
          elapsed={elapsed}
          km=""
          clock={form.start}
          nameSlot={
            <InlineText
              value={form.name === "" ? null : form.name}
              placeholder={t("trip.untitled", { number })}
              label={t("details.name")}
              onChange={set("name")}
              look="label"
              // Not named yet: grey, as every field not given on this screen.
              placeholderTone="muted"
              maxLength={DETAIL_MAX.name}
              testID="manual-name-input"
            />
          }
          elapsedSlot={
            // Hours, minutes and seconds, each typed in place; an empty part is a grey "00", the
            // colons are black.
            <View style={{ flexDirection: "row", alignItems: "center" }} testID="manual-duration">
              {(["hours", "minutes", "seconds"] as const).map((part, i) => (
                <View key={part} style={{ flexDirection: "row", alignItems: "center" }}>
                  {i > 0 ? (
                    // The font of the fields, with no line height either, so the colons stand
                    // on the line of the digits, typed or not.
                    <PlainText
                      style={{
                        fontSize: theme.fontSize["4xl"],
                        fontWeight: theme.fontWeight.bold,
                        color: theme.c.text,
                      }}
                    >
                      :
                    </PlainText>
                  ) : null}
                  <InlineText
                    value={parts[part] === "" ? null : parts[part]}
                    placeholder={part === "hours" ? "0" : "00"}
                    label={t(`duration.part.${part}`)}
                    onChange={setPart(part)}
                    keyboardType="number-pad"
                    look="display"
                    maxLength={2}
                    testID={`manual-duration-${part}`}
                  />
                </View>
              ))}
            </View>
          }
          kmSlot={
            // The width of a space between the number and "km", as in "0.0 km" of a trip; the
            // number grey until it is typed, "km" always black.
            <View style={{ flexDirection: "row", alignItems: "baseline", gap: 6 }}>
              <InlineText
                value={form.km === "" ? null : form.km}
                placeholder="0.0"
                label={t("manual.km")}
                onChange={set("km")}
                keyboardType="decimal-pad"
                look="km"
                maxLength={8}
                testID="manual-km-input"
              />
              <Text variant="title">{t("manual.km.unit")}</Text>
            </View>
          }
          clockSlot={startPicker(<Text tone="muted">{form.start}</Text>)}
        />
        {widgetProblem ? (
          <View style={{ paddingHorizontal: theme.space[4] }}>
            <Text variant="caption" tone="danger" testID="manual-widget-problem">
              {widgetProblem}
            </Text>
          </View>
        ) : null}
      </View>

      <View>
        {typeRow("from", t("details.from"))}
        {typeRow("to", t("details.to"))}
      </View>

      <View>
        {dateRow()}
        <DetailRow
          label={t("trip.time")}
          error={
            problem("start") ??
            (result.problems.duration === "end_future" ? problem("duration") : undefined)
          }
          testID="manual-time"
        >
          <Text tone="muted">{times}</Text>
        </DetailRow>
      </View>

      {/* The note, as on a trip's screen: down to the bottom, growing with its text. */}
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
          value={form.note === "" ? null : form.note}
          // No placeholder: an empty block, a tap anywhere in it and the note is typed.
          placeholder=""
          label={t("details.note")}
          onChange={set("note")}
          look="note"
          maxLength={DETAIL_MAX.note}
          testID="manual-note"
        />
        {problem("note") ? (
          <Text variant="caption" tone="danger">
            {problem("note")}
          </Text>
        ) : null}
      </View>
    </Screen>
  );
}
