import { Text, useTheme } from "@obop/ui-native";
import { View } from "react-native";
import { t } from "../i18n/index.ts";

/** The round avatar: no higher than the line of the title, so the header keeps its height. */
const AVATAR = 28;

/**
 * PLACEHOLDER for stage 2: the driver at the top right of the home screen, their name and their
 * avatar (here "User" and its first letter in a circle). Stage 1 has no sign-in and no profile, so
 * it only shows where they will stand; it does nothing on a tap.
 */
export function UserBadge() {
  const theme = useTheme();
  const name = t("home.user");
  return (
    <View
      accessible
      accessibilityLabel={name}
      style={{ flexDirection: "row", alignItems: "center", gap: theme.space[2] }}
      testID="user"
    >
      <Text tone="muted" numberOfLines={1}>
        {name}
      </Text>
      <View
        style={{
          width: AVATAR,
          height: AVATAR,
          borderRadius: AVATAR / 2,
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: theme.c.accentSoft,
        }}
      >
        <Text variant="label" tone="accent">
          {name.slice(0, 1).toUpperCase()}
        </Text>
      </View>
    </View>
  );
}
