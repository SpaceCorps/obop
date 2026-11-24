import { Button, Field, Text, useInsets, useTheme } from "@obop/ui-native";
import { useEffect, useState } from "react";
import { KeyboardAvoidingView, Modal, Platform, ScrollView, View } from "react-native";
import { DETAIL_MAX, detailsProblems, type TripDetails } from "../core/details.ts";
import { t } from "../i18n/index.ts";

const EMPTY: TripDetails = { name: null, from: null, to: null, note: null };

/**
 * The window of the details of a trip, over the screen, from the bottom: Name, From, To and Note,
 * all optional, and Cancel or the action. The screen of a trip opens it to change them (D6: "Trip
 * details", the fields as stored, Save). Made for Stop in D4b, which no longer opens it: a trip
 * ends after a dialog and is named afterwards. The back button of Android is Cancel. Its fields are
 * set from `initial` each time it opens, so what was typed before a Cancel is not kept.
 */
export function DetailsSheet({
  visible,
  title,
  initial = EMPTY,
  confirmLabel,
  busy,
  failedText,
  onCancel,
  onConfirm,
  testID,
}: {
  visible: boolean;
  title: string;
  /** The fields when the window opens; empty when not given. */
  initial?: TripDetails;
  confirmLabel: string;
  busy: boolean;
  /** The last action failed: said under the fields, and the window stays open with what was typed. */
  failedText: string | null;
  onCancel: () => void;
  onConfirm: (details: TripDetails) => void;
  /** The window's own test id; its fields and buttons are `details-name` ... `details-confirm`. */
  testID: string;
}) {
  const theme = useTheme();
  const insets = useInsets();
  const [name, setName] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [note, setNote] = useState("");

  useEffect(() => {
    if (!visible) return;
    setName(initial.name ?? "");
    setFrom(initial.from ?? "");
    setTo(initial.to ?? "");
    setNote(initial.note ?? "");
    // Only when it opens: a change of `initial` while it is open (the list read again) keeps what
    // is being typed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible]);

  const details: TripDetails = { name, from, to, note };
  const problems = detailsProblems(details);
  const tooLong = (field: keyof typeof DETAIL_MAX) =>
    problems.includes(field) ? t("details.too_long", { max: DETAIL_MAX[field] }) : undefined;

  return (
    <Modal visible={visible} transparent animationType="slide" onRequestClose={onCancel}>
      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : undefined}
        style={{ flex: 1, justifyContent: "flex-end", backgroundColor: "rgba(0,0,0,0.4)" }}
      >
        <View
          testID={testID}
          style={{
            maxHeight: "90%",
            backgroundColor: theme.c.surface,
            borderTopLeftRadius: theme.radius.lg,
            borderTopRightRadius: theme.radius.lg,
            paddingBottom: insets.bottom,
          }}
        >
          <ScrollView
            keyboardShouldPersistTaps="handled"
            contentContainerStyle={{ gap: theme.space[3], padding: theme.space[4] }}
          >
            <Text variant="title">{title}</Text>
            <Field
              label={t("details.name")}
              value={name}
              onChangeText={setName}
              maxLength={DETAIL_MAX.name}
              error={tooLong("name")}
              testID="details-name"
            />
            <Field
              label={t("details.from")}
              value={from}
              onChangeText={setFrom}
              maxLength={DETAIL_MAX.from}
              error={tooLong("from")}
              testID="details-from"
            />
            <Field
              label={t("details.to")}
              value={to}
              onChangeText={setTo}
              maxLength={DETAIL_MAX.to}
              error={tooLong("to")}
              testID="details-to"
            />
            <Field
              label={t("details.note")}
              value={note}
              onChangeText={setNote}
              multiline
              maxLength={DETAIL_MAX.note}
              error={tooLong("note")}
              testID="details-note"
            />
            {failedText === null ? null : (
              <Text tone="danger" testID="details-failed">
                {failedText}
              </Text>
            )}
          </ScrollView>
          <View
            style={{
              flexDirection: "row",
              gap: theme.space[3],
              paddingHorizontal: theme.space[4],
              paddingVertical: theme.space[3],
            }}
          >
            <View style={{ flex: 1 }}>
              <Button
                label={t("common.cancel")}
                variant="secondary"
                disabled={busy}
                onPress={onCancel}
                testID="details-cancel"
              />
            </View>
            <View style={{ flex: 1 }}>
              <Button
                label={confirmLabel}
                busy={busy}
                disabled={problems.length > 0}
                onPress={() => onConfirm(details)}
                testID="details-confirm"
              />
            </View>
          </View>
        </View>
      </KeyboardAvoidingView>
    </Modal>
  );
}
