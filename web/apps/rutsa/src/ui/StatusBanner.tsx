import { Text, useTheme, type Insets } from "@obop/ui-native";
import { Pressable, View } from "react-native";
import { t } from "../i18n/index.ts";
import { openLocationSettings, openSettings } from "../native/permissions.ts";
import { bannerText, settingsFor, type Problem } from "./status.ts";

/**
 * The status banner at the top of the screens: one short line that says what is off
 * (`bannerText` of `ui/status.ts`), Open settings, and a close button at the right, in the middle
 * of the banner's height; as low as one line of text allows. It takes the space of the system bar
 * at the top (`insets.top`); the screen under it gets none (`App.tsx`). Built here from the parts
 * of `@obop/ui-native`, whose `Banner` has a title and no close button.
 */
export function StatusBanner({
  problems,
  insets,
  onClose,
}: {
  problems: readonly Problem[];
  insets: Insets;
  onClose: () => void;
}) {
  const theme = useTheme();
  const sig = theme.c.warning;
  const key = bannerText(problems);
  if (key === null) return null;
  const sentence = t(key);
  const settings = settingsFor(problems);
  return (
    <View
      testID="status-banner"
      style={{
        flexDirection: "row",
        alignItems: "center",
        gap: theme.space[3],
        paddingTop: insets.top + theme.space[2],
        paddingBottom: theme.space[2],
        paddingLeft: insets.left + theme.space[4],
        paddingRight: insets.right + theme.space[3],
        backgroundColor: sig.soft,
        borderBottomWidth: 2,
        borderBottomColor: sig.fill,
      }}
    >
      <View accessible accessibilityRole="alert" accessibilityLabel={sentence} style={{ flex: 1 }}>
        <Text color={sig.ink} testID="status-text">
          {sentence}
        </Text>
      </View>
      <Pressable
        accessibilityRole="button"
        hitSlop={12}
        onPress={() => void (settings === "location" ? openLocationSettings() : openSettings())}
        testID="status-settings"
      >
        <Text variant="label" color={sig.ink}>
          {t("permission.settings")}
        </Text>
      </Pressable>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={t("status.close")}
        hitSlop={12}
        onPress={onClose}
        testID="status-close"
      >
        <Text variant="title" color={sig.ink}>
          ×
        </Text>
      </Pressable>
    </View>
  );
}
