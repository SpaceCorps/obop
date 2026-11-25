import { Banner, Button, Screen, Text, useTheme, useWords } from "@obop/ui-native";
import { useState } from "react";
import { Alert, View } from "react-native";
import { DETAIL_MAX } from "../../core/details.ts";
import {
  FAST_KMH,
  MAX_KM,
  checkManual,
  type ManualField,
  type ManualForm,
  type ManualProblem,
} from "../../core/manual.ts";
import { dayKey, formatClock, formatDay, momentOf, phoneTimeZone } from "../../core/present.ts";
import type { TripRepository } from "../../core/repository.ts";
import { LANGUAGE, t } from "../../i18n/index.ts";
import { addManualTrip } from "../../native/actions.ts";
import { DetailRow } from "../DetailRow.tsx";
import { useFillToFooter } from "../fill.ts";
import { useTrips } from "../hooks.ts";
import { InlineText } from "../InlineText.tsx";
import { IosPicker, pickerInRow, pickOnAndroid, type PickMode } from "../pick.tsx";
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

/** `08:30` as hours and minutes; `null` when not chosen yet. */
function clockOf(text: string): { hour: number; minute: number } | null {
  const match = /^(\d{2}):(\d{2})$/.exec(text);
  return match ? { hour: Number(match[1]), minute: Number(match[2]) } : null;
}

/**
 * A trip added by hand (D7, BR-28), in the look of a trip's screen: the title "Manual add", then
 * Name, From and To typed in place; then Date, Start and End chosen with the pickers of the system
 * (an end before the start is on the next day, said under it) and Distance typed in place; then
 * Note, down to the footer; Back and Save in the footer. Save checks it all (`checkManual`): each
 * problem is said under its row, and an average speed over `FAST_KMH` asks first. The trip is
 * stored as manual and the screen goes back. Reachable without location (US-18 AC1).
 */
export function AddTrip({ store, nav }: { store: TripRepository; nav: Nav }) {
  const theme = useTheme();
  const shared = useWords();
  const { trips } = useTrips(store);
  const note = useFillToFooter();
  const zone = phoneTimeZone();
  const [form, setForm] = useState<ManualForm>(() => ({
    name: "",
    date: dayKey(Date.now(), zone),
    start: "",
    end: "",
    km: "",
    from: "",
    to: "",
    note: "",
  }));
  // Problems are said once Save was pressed, and from then on as the driver types.
  const [tried, setTried] = useState(false);
  const [saving, setSaving] = useState(false);
  const [failed, setFailed] = useState(false);

  const now = Date.now();
  const result = checkManual(form, now, zone, trips);
  const set = (field: ManualField) => (text: string) => setForm((f) => ({ ...f, [field]: text }));
  const problem = (field: ManualField): string | undefined => {
    const found = result.problems[field];
    return tried && found ? problemText(found, field) : undefined;
  };

  /** The moment a picker opens on: the chosen date and time; a time not chosen yet opens on now. */
  const pickerValue = (field: "date" | "start" | "end"): Date => {
    const clock =
      field === "date"
        ? { hour: 12, minute: 0 }
        : (clockOf(form[field]) ?? clockOf(formatClock(now, "en-GB", zone)));
    return new Date(momentOf(form.date, clock?.hour ?? 12, clock?.minute ?? 0, zone));
  };
  const chosen = (field: "date" | "start" | "end") => (date: Date) =>
    set(field)(
      field === "date" ? dayKey(date.getTime(), zone) : formatClock(date.getTime(), "en-GB", zone),
    );

  /** A row whose value a picker chooses: the dialog of Android on a tap, the picker of iOS in it. */
  const pickRow = (field: "date" | "start" | "end", label: string, hint?: string) => {
    const mode: PickMode = field === "date" ? "date" : "time";
    const value = pickerValue(field);
    const max = field === "date" ? new Date(now) : undefined;
    const otherYear = form.date.slice(0, 4) !== dayKey(now, zone).slice(0, 4);
    const text =
      field === "date" ? formatDay(value.getTime(), LANGUAGE, zone, otherYear) : form[field];
    return (
      <DetailRow
        label={label}
        error={problem(field)}
        hint={hint}
        {...(pickerInRow
          ? {}
          : {
              onPress: () => {
                void pickOnAndroid(mode, value, max).then((date) => {
                  if (date) chosen(field)(date);
                });
              },
            })}
        testID={`manual-${field}`}
      >
        {pickerInRow ? (
          <IosPicker
            mode={mode}
            value={value}
            {...(max ? { maximumDate: max } : {})}
            onPick={chosen(field)}
          />
        ) : (
          <Text tone="muted">{text === "" ? t("manual.choose") : text}</Text>
        )}
      </DetailRow>
    );
  };

  /** A row whose value is typed in place. */
  const typeRow = (
    field: "name" | "from" | "to" | "km",
    label: string,
    placeholder: string,
    maxLength: number,
  ) => (
    <DetailRow label={label} error={problem(field)} testID={`manual-${field}`}>
      <InlineText
        value={form[field] === "" ? null : form[field]}
        placeholder={placeholder}
        label={label}
        onChange={set(field)}
        look="value"
        maxLength={maxLength}
        {...(field === "km" ? { keyboardType: "decimal-pad" as const } : {})}
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

  const half = { flex: 1 } as const;
  return (
    <Screen
      title={t("manual.title")}
      footer={
        <View ref={note.footerRef} style={{ flexDirection: "row", gap: theme.space[3] }}>
          <View style={half}>
            <Button
              label={shared.back}
              variant="secondary"
              disabled={saving}
              onPress={nav.back}
              testID="manual-back"
            />
          </View>
          <View style={half}>
            <Button label={t("common.save")} busy={saving} onPress={save} testID="manual-save" />
          </View>
        </View>
      }
    >
      {failed ? <Banner tone="danger" title={t("manual.failed")} testID="manual-failed" /> : null}

      <View>
        {typeRow("name", t("details.name"), t("manual.name.placeholder"), DETAIL_MAX.name)}
        {typeRow("from", t("details.from"), t("trip.none"), DETAIL_MAX.from)}
        {typeRow("to", t("details.to"), t("trip.none"), DETAIL_MAX.to)}
      </View>

      <View>
        {pickRow("date", t("manual.date"))}
        {pickRow("start", t("manual.start"))}
        {pickRow("end", t("manual.end"), result.nextDay ? t("manual.next_day") : undefined)}
        {typeRow("km", t("manual.km"), "0.0", 8)}
      </View>

      {/* The note, as on a trip's screen: down to the footer, growing with its text. */}
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
          placeholder={t("manual.note.placeholder")}
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
