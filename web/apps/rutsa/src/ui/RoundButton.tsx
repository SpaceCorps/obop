import { useTheme } from "@obop/ui-native";
import { ActivityIndicator, Pressable, View } from "react-native";
import { ON_FILL } from "./ScreenHeader.tsx";

/**
 * The big round button of the home screen (Start, Stop, Resume), drawn as the record button of a
 * camera: a light ring round a darker disc with a red circle in it (Start, Resume) or a red square
 * (Stop). It shows no word; its word is said to a screen reader (`label`). It stands at the right
 * of the current widget of the home screen, with the small round Pause and Restart at its left (as
 * high as it together).
 */
export const ROUND_SIZE = 72;

/**
 * The red of recording: the circle and the square of the round button, a small Stop and the dot of
 * Recording.
 */
export const RECORD_RED = "#ff3b30";

/**
 * Delete trip: the red of recording at 80 % over the dark ground of the screens, as bright to the
 * eye as the square of Stop on its grey disc.
 */
export const DELETE_RED = "#cf3229";

/** The light ring round the round buttons of a recording, big and small. */
export const RING = "rgba(255, 255, 255, 0.4)";

export function RoundButton({
  label,
  shape,
  size = ROUND_SIZE,
  busy = false,
  hint,
  onPress,
  testID,
}: {
  label: string;
  /** A red circle (Start, Resume) or a red square (Stop). */
  shape: "circle" | "square";
  /** Its diameter. */
  size?: number;
  busy?: boolean;
  hint?: string;
  onPress: () => void;
  testID?: string;
}) {
  const theme = useTheme();
  const mark =
    shape === "circle"
      ? { width: size * 0.8, height: size * 0.8, borderRadius: size * 0.4 }
      : { width: size * 0.42, height: size * 0.42, borderRadius: size * 0.1 };
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityHint={hint}
      accessibilityState={{ disabled: busy, busy }}
      disabled={busy}
      onPress={onPress}
      testID={testID}
      style={({ pressed }) => ({
        width: size,
        height: size,
        borderRadius: size / 2,
        alignItems: "center",
        justifyContent: "center",
        alignSelf: "center",
        borderWidth: 2,
        borderColor: RING,
        backgroundColor: theme.c.surface3,
        opacity: busy ? 0.5 : pressed ? 0.8 : 1,
      })}
    >
      <View style={{ ...mark, backgroundColor: RECORD_RED }} />
      {busy ? <ActivityIndicator color={ON_FILL} style={{ position: "absolute" }} /> : null}
    </Pressable>
  );
}
