import { ActivityIndicator, Pressable, View } from "react-native";
import { useTheme } from "../theme.tsx";
import { Text } from "./Text.tsx";

export type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";

export type ButtonProps = {
  label: string;
  onPress: () => void;
  variant?: ButtonVariant;
  /** `big` is the round Start button of the home screen. */
  size?: "md" | "big";
  disabled?: boolean;
  /** A call is running: the button is announced as busy and ignores presses. */
  busy?: boolean;
  /** What the button does, read after its name by a screen reader. */
  hint?: string;
  /**
   * The button is one choice of a group (a radio button): `true` for the chosen one. A screen reader
   * says "selected" or "not selected", and a check mark in the label shows it without colour.
   */
  selected?: boolean;
  testID?: string;
};

export function Button({
  label,
  onPress,
  variant = "primary",
  size = "md",
  disabled = false,
  busy = false,
  hint,
  selected,
  testID,
}: ButtonProps) {
  const t = useTheme();
  const off = disabled || busy;
  const fill =
    variant === "primary"
      ? t.c.accentFill
      : variant === "danger"
        ? t.c.danger.fill
        : variant === "secondary"
          ? t.c.surface
          : "transparent";
  const ink =
    variant === "primary"
      ? t.c.onAccent
      : variant === "danger"
        ? t.c.onDanger
        : variant === "secondary"
          ? t.c.text
          : t.c.accentInk;
  const big = size === "big";
  return (
    <Pressable
      accessibilityRole={selected === undefined ? "button" : "radio"}
      accessibilityLabel={label}
      accessibilityHint={hint}
      accessibilityState={{
        disabled: off,
        busy,
        ...(selected === undefined ? {} : { checked: selected }),
      }}
      disabled={off}
      onPress={onPress}
      testID={testID}
      android_ripple={{ color: t.c.accentSoft }}
      style={({ pressed }) => ({
        minHeight: big ? t.control.big : t.control.height,
        minWidth: big ? t.control.big : t.control.minTouch,
        borderRadius: big ? t.control.big / 2 : t.radius.lg,
        paddingHorizontal: t.space[5],
        paddingVertical: t.space[2],
        alignItems: "center",
        justifyContent: "center",
        alignSelf: big ? "center" : "stretch",
        backgroundColor: fill,
        borderWidth: variant === "secondary" ? 1 : 0,
        borderColor: t.c.borderControl,
        opacity: off ? 0.5 : pressed ? 0.85 : 1,
      })}
    >
      <View style={{ flexDirection: "row", alignItems: "center", gap: t.space[2] }}>
        {busy ? <ActivityIndicator color={ink} /> : null}
        <Text variant={big ? "title" : "label"} color={ink}>
          {selected === true ? `✓ ${label}` : label}
        </Text>
      </View>
    </Pressable>
  );
}
