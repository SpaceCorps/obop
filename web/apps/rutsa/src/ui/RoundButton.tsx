import { Text, useTheme } from "@obop/ui-native";
import { ActivityIndicator, Pressable, View } from "react-native";

/**
 * The big round button of the home screen (Start, Stop, Resume): `Button size="big"` of
 * `@obop/ui-native` with a smaller circle, so the footer leaves room for one more recent trip. The
 * size of `big` is a shared token (176), and the other apps keep it.
 */
export const ROUND_SIZE = 144;

export function RoundButton({
  label,
  danger = false,
  busy = false,
  hint,
  onPress,
  testID,
}: {
  label: string;
  /** Red (Stop) instead of the main colour (Start, Resume). */
  danger?: boolean;
  busy?: boolean;
  hint?: string;
  onPress: () => void;
  testID?: string;
}) {
  const theme = useTheme();
  const fill = danger ? theme.c.danger.fill : theme.c.accentFill;
  const ink = danger ? theme.c.onDanger : theme.c.onAccent;
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityHint={hint}
      accessibilityState={{ disabled: busy, busy }}
      disabled={busy}
      onPress={onPress}
      testID={testID}
      // Not borderless: on Android a borderless ripple takes the place of the background colour.
      android_ripple={{ color: theme.c.accentSoft }}
      style={({ pressed }) => ({
        width: ROUND_SIZE,
        height: ROUND_SIZE,
        borderRadius: ROUND_SIZE / 2,
        alignItems: "center",
        justifyContent: "center",
        alignSelf: "center",
        backgroundColor: fill,
        opacity: busy ? 0.5 : pressed ? 0.85 : 1,
      })}
    >
      <View style={{ flexDirection: "row", alignItems: "center", gap: theme.space[2] }}>
        {busy ? <ActivityIndicator color={ink} /> : null}
        <Text variant="title" color={ink}>
          {label}
        </Text>
      </View>
    </Pressable>
  );
}
