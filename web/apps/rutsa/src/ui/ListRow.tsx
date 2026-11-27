import { Text, useTheme } from "@obop/ui-native";
import { Pressable, View } from "react-native";
import { GROUP_INSET } from "./Group.tsx";
import { Icon } from "./icons.tsx";

/**
 * A row of a list on a card (`Group`), as the lists of iOS: its title (and a line under it) at the
 * left, its value at the right, and, when a tap opens its page (`onPress`), an arrow at the end.
 * The look of `Row` of `@obop/ui-native` without its ground and its line, which the card draws.
 */
export function ListRow({
  title,
  subtitle,
  value,
  onPress,
  hint,
  testID,
}: {
  title: string;
  /** The line under the title. */
  subtitle?: string | undefined;
  /** The value at the right (a distance). */
  value?: string | undefined;
  /** Opens the row's page; the row then has the arrow. */
  onPress?: () => void;
  hint?: string;
  testID?: string;
}) {
  const theme = useTheme();
  const spoken = [title, subtitle, value].filter(Boolean).join(", ");
  const content = (
    <View
      style={{
        minHeight: theme.control.height,
        flexDirection: "row",
        alignItems: "center",
        gap: theme.space[3],
        paddingVertical: theme.space[2],
      }}
    >
      <View style={{ flex: 1, minWidth: "30%" }}>
        <Text variant="label">{title}</Text>
        {subtitle ? (
          <Text variant="caption" tone="muted">
            {subtitle}
          </Text>
        ) : null}
      </View>
      {/* A long value wraps inside its share of the row; it never squeezes the title to nothing. */}
      {value ? (
        <View style={{ flexShrink: 1, maxWidth: "65%" }}>
          <Text tone="muted">{value}</Text>
        </View>
      ) : null}
      {onPress ? <Icon name="forward" color={theme.c.textSubtle} size={14} /> : null}
    </View>
  );
  const frame = { paddingHorizontal: GROUP_INSET } as const;
  if (!onPress) {
    return (
      <View accessible accessibilityLabel={spoken} testID={testID} style={frame}>
        {content}
      </View>
    );
  }
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={spoken}
      accessibilityHint={hint}
      onPress={onPress}
      testID={testID}
      android_ripple={{ color: theme.c.accentSoft }}
      style={({ pressed }) => ({ ...frame, opacity: pressed ? 0.85 : 1 })}
    >
      {content}
    </Pressable>
  );
}
