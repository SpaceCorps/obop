import { Text, useTheme } from "@obop/ui-native";
import { ActivityIndicator, Pressable, View } from "react-native";
import { ON_FILL } from "./ScreenHeader.tsx";

/**
 * The big round button of the home screen (Start, Stop, Resume): `Button size="big"` of
 * `@obop/ui-native` with a smaller circle and word, at the right of the current widget of the
 * home screen, with the small round Pause and Restart at its left (as high as it together);
 * small enough to leave the big time its room at their left. The size of `big` is a shared token (176), and the other
 * apps keep it.
 */
export const ROUND_SIZE = 72;

export function RoundButton({
  label,
  size = ROUND_SIZE,
  danger = false,
  busy = false,
  hint,
  onPress,
  testID,
}: {
  label: string;
  /** Its diameter. */
  size?: number;
  /** Red (Stop) instead of the main colour (Start, Resume). */
  danger?: boolean;
  busy?: boolean;
  hint?: string;
  onPress: () => void;
  testID?: string;
}) {
  const theme = useTheme();
  const fill = danger ? theme.c.danger.fill : theme.c.accentFill;
  // White on the main colour and on red, in the dark theme too (its `onAccent` and `onDanger`
  // are nearly black), as the filled buttons of the header.
  const ink = ON_FILL;
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
        width: size,
        height: size,
        borderRadius: size / 2,
        alignItems: "center",
        justifyContent: "center",
        alignSelf: "center",
        backgroundColor: fill,
        opacity: busy ? 0.5 : pressed ? 0.85 : 1,
      })}
    >
      <View style={{ flexDirection: "row", alignItems: "center", gap: theme.space[2] }}>
        {busy ? <ActivityIndicator color={ink} /> : null}
        <Text variant="heading" color={ink} numberOfLines={1} adjustsFontSizeToFit>
          {label}
        </Text>
      </View>
    </Pressable>
  );
}
