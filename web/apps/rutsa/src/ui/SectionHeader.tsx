import { Text } from "@obop/ui-native";
import { Pressable, View } from "react-native";
import { GROUP_INSET } from "./Group.tsx";

/**
 * The header of a group of rows, as in iOS: grey, over its card, as far in from the sides as the
 * words of the rows (`GROUP_INSET`); an action at its right in the main colour ("Show more"). The
 * same for every list of the app (History, Record).
 */
export function SectionHeader({
  title,
  action,
}: {
  title: string;
  action?: { label: string; onPress: () => void; testID?: string };
}) {
  return (
    <View style={{ flexDirection: "row", alignItems: "center", paddingHorizontal: GROUP_INSET }}>
      <View style={{ flex: 1 }}>
        <Text variant="label" tone="muted" accessibilityRole="header">
          {title}
        </Text>
      </View>
      {action ? (
        <Pressable
          accessibilityRole="button"
          onPress={action.onPress}
          hitSlop={12}
          testID={action.testID}
        >
          <Text variant="label" tone="accent">
            {action.label}
          </Text>
        </Pressable>
      ) : null}
    </View>
  );
}
