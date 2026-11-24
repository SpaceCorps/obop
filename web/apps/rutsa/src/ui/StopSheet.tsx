import { Button, Field, Text, useInsets, useTheme } from "@obop/ui-native";
import { useEffect, useState } from "react";
import { KeyboardAvoidingView, Modal, Platform, ScrollView, View } from "react-native";
import { DETAIL_MAX, detailsProblems, type TripDetails } from "../core/details.ts";
import { t } from "../i18n/index.ts";

/**
 * The window of Stop (D4b): over the home screen, from the bottom, with the details of the trip,
 * all optional (Name, From, To, Note), and Cancel or "Stop trip". It replaces the confirmation of
 * Stop: Cancel keeps recording, "Stop trip" ends the trip with what was typed, also with nothing.
 * The back button of Android is Cancel. Its fields are empty each time it opens.
 */
export function StopSheet({
  visible,
  summary,
  busy,
  failed,
  onCancel,
  onStop,
}: {
  visible: boolean;
  /** The trip as Current shows it: its time and km. */
  summary: string;
  busy: boolean;
  /** The last "Stop trip" failed; the window stays open with what was typed. */
  failed: boolean;
  onCancel: () => void;
  onStop: (details: TripDetails) => void;
}) {
  const theme = useTheme();
  const insets = useInsets();
  const [name, setName] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [note, setNote] = useState("");

  useEffect(() => {
    if (!visible) return;
    setName("");
    setFrom("");
    setTo("");
    setNote("");
  }, [visible]);

  const details: TripDetails = { name, from, to, note };
  const problems = detailsProblems(details);
  const tooLong = (field: keyof typeof DETAIL_MAX) =>
    problems.includes(field) ? t("stop.too_long", { max: DETAIL_MAX[field] }) : undefined;

  return (
    <Modal visible={visible} transparent animationType="slide" onRequestClose={onCancel}>
      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : undefined}
        style={{ flex: 1, justifyContent: "flex-end", backgroundColor: "rgba(0,0,0,0.4)" }}
      >
        <View
          testID="stop-sheet"
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
            <Text variant="title">{t("stop.title")}</Text>
            <Text tone="muted">{summary}</Text>
            <Text tone="muted">{t("stop.hint")}</Text>
            <Field
              label={t("stop.name")}
              value={name}
              onChangeText={setName}
              maxLength={DETAIL_MAX.name}
              error={tooLong("name")}
              testID="stop-name"
            />
            <Field
              label={t("stop.from")}
              value={from}
              onChangeText={setFrom}
              maxLength={DETAIL_MAX.from}
              error={tooLong("from")}
              testID="stop-from"
            />
            <Field
              label={t("stop.to")}
              value={to}
              onChangeText={setTo}
              maxLength={DETAIL_MAX.to}
              error={tooLong("to")}
              testID="stop-to"
            />
            <Field
              label={t("stop.note")}
              value={note}
              onChangeText={setNote}
              multiline
              maxLength={DETAIL_MAX.note}
              error={tooLong("note")}
              testID="stop-note"
            />
            {failed ? (
              <Text tone="danger" testID="stop-failed">
                {t("stop.failed")}
              </Text>
            ) : null}
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
                testID="stop-cancel"
              />
            </View>
            <View style={{ flex: 1 }}>
              <Button
                label={t("stop.confirm")}
                variant="danger"
                busy={busy}
                disabled={problems.length > 0}
                onPress={() => onStop(details)}
                testID="stop-confirm"
              />
            </View>
          </View>
        </View>
      </KeyboardAvoidingView>
    </Modal>
  );
}
