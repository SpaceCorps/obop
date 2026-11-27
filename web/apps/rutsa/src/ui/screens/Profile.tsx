import { Screen, Text, useTheme } from "@obop/ui-native";
import { View } from "react-native";
import { t } from "../../i18n/index.ts";
import { HEADER_BUTTON, ScreenHeader } from "../ScreenHeader.tsx";

const AVATAR = 88;

/**
 * The tab Profile: a PLACEHOLDER for stage 2, which puts the driver's profile here with the
 * settings (the lock of BR-35, the export, the theme and the language). Stage 1 has no sign-in:
 * the avatar shows the first letter of "User".
 */
export function Profile() {
  const theme = useTheme();
  const name = t("home.user");
  return (
    <Screen
      floatingHeaderHeight={HEADER_BUTTON}
      title={t("tabs.profile")}
      titleSlot={<ScreenHeader title={t("tabs.profile")} />}
    >
      <View style={{ alignItems: "center", gap: theme.space[3], paddingTop: theme.space[5] }}>
        <View
          testID="profile-avatar"
          style={{
            width: AVATAR,
            height: AVATAR,
            borderRadius: AVATAR / 2,
            alignItems: "center",
            justifyContent: "center",
            backgroundColor: theme.c.accentSoft,
          }}
        >
          <Text variant="display" tone="accent">
            {name.slice(0, 1).toUpperCase()}
          </Text>
        </View>
        <Text variant="title" testID="profile-name">
          {name}
        </Text>
        <Text tone="muted" testID="profile-later">
          {t("profile.later")}
        </Text>
      </View>
    </Screen>
  );
}
