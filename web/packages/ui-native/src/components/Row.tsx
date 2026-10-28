import type { ReactNode } from "react";
import { Pressable, View } from "react-native";
import { useTheme } from "../theme.tsx";
import { Text } from "./Text.tsx";

export type RowProps = {
  title: string;
  /** The line under the title. */
  subtitle?: string;
  /** The value on the right (a distance, a state). */
  value?: string;
  /** Something on the right of the value, such as a state badge. */
  trailing?: ReactNode;
  /** A row with `onPress` is a button; without it the row is plain text. */
  onPress?: () => void;
  hint?: string;
  testID?: string;
};

export function Row({ title, subtitle, value, trailing, onPress, hint, testID }: RowProps) {
  const t = useTheme();
  const spoken = [title, subtitle, value].filter(Boolean).join(", ");
  const content = (
    <View
      style={{
        minHeight: t.control.height,
        flexDirection: "row",
        alignItems: "center",
        gap: t.space[3],
        paddingVertical: t.space[2],
      }}
    >
      <View style={{ flex: 1 }}>
        <Text variant="label">{title}</Text>
        {subtitle ? (
          <Text variant="caption" tone="muted">
            {subtitle}
          </Text>
        ) : null}
      </View>
      {value ? <Text tone="muted">{value}</Text> : null}
      {trailing}
    </View>
  );
  const frame = {
    paddingHorizontal: t.space[4],
    backgroundColor: t.c.surface,
    borderBottomWidth: 1,
    borderBottomColor: t.c.border,
  } as const;
  if (!onPress)
    return (
      <View accessible accessibilityLabel={spoken} testID={testID} style={frame}>
        {content}
      </View>
    );
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={spoken}
      accessibilityHint={hint}
      onPress={onPress}
      testID={testID}
      android_ripple={{ color: t.c.accentSoft }}
      style={({ pressed }) => ({ ...frame, opacity: pressed ? 0.85 : 1 })}
    >
      {content}
    </Pressable>
  );
}
