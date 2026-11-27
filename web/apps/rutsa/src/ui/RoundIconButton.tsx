import { useTheme } from "@obop/ui-native";
import { ActivityIndicator, Pressable } from "react-native";
import { Icon, type IconName } from "./icons.tsx";
import { RECORD_RED, RING } from "./RoundButton.tsx";
import { ON_FILL } from "./ScreenHeader.tsx";

/** The size of a small round button of a recording; two and their gap are as high as the big one. */
export const ROUND_ICON_SIZE = 32;

/**
 * A small round button of the record widget with one symbol (Pause, Restart, Stop while paused), at
 * the left of the big round one, in its look: the light ring round the dark disc, a white symbol;
 * `danger` makes the symbol the red of recording (Stop). Its word is said to a screen reader
 * (`label`).
 */
export function RoundIconButton({
  icon,
  label,
  danger = false,
  busy = false,
  onPress,
  testID,
}: {
  icon: IconName;
  label: string;
  danger?: boolean;
  busy?: boolean;
  onPress: () => void;
  testID?: string;
}) {
  const theme = useTheme();
  const ink = danger ? RECORD_RED : ON_FILL;
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled: busy, busy }}
      disabled={busy}
      onPress={onPress}
      hitSlop={6}
      testID={testID}
      style={({ pressed }) => ({
        width: ROUND_ICON_SIZE,
        height: ROUND_ICON_SIZE,
        borderRadius: ROUND_ICON_SIZE / 2,
        alignItems: "center",
        justifyContent: "center",
        borderWidth: 1.5,
        borderColor: RING,
        backgroundColor: theme.c.surface3,
        opacity: busy ? 0.5 : pressed ? 0.7 : 1,
      })}
    >
      {busy ? (
        <ActivityIndicator size="small" color={ink} />
      ) : (
        <Icon name={icon} color={ink} size={16} />
      )}
    </Pressable>
  );
}
