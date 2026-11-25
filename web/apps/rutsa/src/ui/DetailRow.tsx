import { Text, useTheme } from "@obop/ui-native";
import type { ReactNode } from "react";
import { Pressable, View } from "react-native";

/**
 * A row of the details of a trip (D6, D7): its label at the left, its value at the right (a text
 * changed in place, or the value a tap chooses), and a problem under it in red. The look of `Row`
 * of `@obop/ui-native`, which takes only text.
 */
export function DetailRow({
  label,
  children,
  error,
  hint,
  onPress,
  testID,
}: {
  label: string;
  /** The value: an `InlineText`, or a text that `onPress` chooses. */
  children: ReactNode;
  error?: string | undefined;
  /** A note under it in grey ("Ends the next day."); a problem takes its place. */
  hint?: string | undefined;
  /** The row is a button that chooses its value (a date, a time). */
  onPress?: () => void;
  testID?: string;
}) {
  const theme = useTheme();
  const frame = {
    paddingVertical: theme.space[2],
    paddingHorizontal: theme.space[4],
    backgroundColor: theme.c.surface,
    borderBottomWidth: 1,
    borderBottomColor: theme.c.border,
  } as const;
  const content = (
    <>
      <View
        style={{
          minHeight: theme.control.height - 2 * theme.space[2],
          flexDirection: "row",
          alignItems: "center",
          gap: theme.space[3],
        }}
      >
        <Text variant="label">{label}</Text>
        <View style={{ flex: 1, flexDirection: "row", justifyContent: "flex-end" }}>
          {children}
        </View>
      </View>
      {error ? (
        <Text variant="caption" tone="danger" testID={testID ? `${testID}-error` : undefined}>
          {error}
        </Text>
      ) : hint ? (
        <Text variant="caption" tone="muted">
          {hint}
        </Text>
      ) : null}
    </>
  );
  if (!onPress) {
    return (
      <View style={frame} testID={testID}>
        {content}
      </View>
    );
  }
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      onPress={onPress}
      android_ripple={{ color: theme.c.accentSoft }}
      testID={testID}
      style={({ pressed }) => ({ ...frame, opacity: pressed ? 0.85 : 1 })}
    >
      {content}
    </Pressable>
  );
}
