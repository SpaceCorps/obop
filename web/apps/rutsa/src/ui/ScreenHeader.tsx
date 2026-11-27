import { Text, useTheme } from "@obop/ui-native";
import { ActivityIndicator, Pressable, View } from "react-native";
import { Icon, type IconName } from "./icons.tsx";

/** The symbols of the round buttons of a recording: white in both themes. */
export const ON_FILL = "#ffffff";

/** The height of a button of the header, and so of the header. */
export const HEADER_BUTTON = 40;

/** A button at a corner of the header: round with a symbol, or a pill with a word ("Edit"). */
export type HeaderButton = (
  | { icon: IconName; text?: undefined }
  | { text: string; icon?: undefined }
) & {
  /** What it does, for a screen reader: Back, Edit, Save. */
  label: string;
  onPress?: () => void;
  /**
   * `filled`: the ground and the ring of the main colour, the symbol cut out of it (Add, and the
   * action of the screen, as Save); `outline` (the default): no ground, a ring and a symbol of the main colour.
   */
  look?: "filled" | "outline";
  /** Shown greyed, and a tap does nothing. */
  disabled?: boolean;
  /** A PLACEHOLDER of stage 2: shown as it will be, and a tap does nothing yet. */
  inactive?: boolean;
  /** Its action runs: a spinner in place of the symbol. */
  busy?: boolean;
  testID?: string;
};

function CornerButton({ button }: { button: HeaderButton }) {
  const theme = useTheme();
  const filled = button.look === "filled";
  // Filled: the ground and the ring of the main colour (as the ring of an outlined button), the
  // symbol cut out of it: drawn in the ground of the screen the header lies on.
  const ink = filled ? theme.c.bg : theme.c.accentFill;
  const off = button.disabled === true || button.busy === true || button.inactive === true;
  const pill = button.text !== undefined;
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={button.label}
      accessibilityState={{ disabled: off, busy: button.busy === true }}
      disabled={off}
      onPress={button.onPress}
      hitSlop={6}
      testID={button.testID}
      style={({ pressed }) => ({
        height: HEADER_BUTTON,
        minWidth: HEADER_BUTTON,
        paddingHorizontal: pill ? theme.space[4] : 0,
        borderRadius: HEADER_BUTTON / 2,
        alignItems: "center",
        justifyContent: "center",
        borderWidth: 1.5,
        borderColor: theme.c.accentFill,
        backgroundColor: filled ? theme.c.accentFill : "transparent",
        opacity: button.disabled ? 0.35 : pressed ? 0.7 : 1,
      })}
    >
      {button.busy ? (
        <ActivityIndicator size="small" color={ink} />
      ) : pill ? (
        <Text variant="label" color={ink}>
          {button.text}
        </Text>
      ) : (
        <Icon name={button.icon} color={ink} size={20} />
      )}
    </Pressable>
  );
}

/**
 * The header of a screen, in the manner of iOS: the title in the middle and a button at each
 * corner (round with a symbol: Back, Save, Add, Sort; or a pill with a word: Edit, Done), either
 * of which a screen may leave out; the two corners are as wide as each other, so the title stays
 * in the middle all the same. The buttons take the place of the footer of the screens.
 */
export function ScreenHeader({
  title,
  left,
  right,
}: {
  title: string;
  left?: HeaderButton | undefined;
  right?: HeaderButton | undefined;
}) {
  const theme = useTheme();
  const corner = (button: HeaderButton | undefined, side: "left" | "right") => (
    <View
      style={{
        flexGrow: 1,
        flexBasis: 0,
        minWidth: HEADER_BUTTON + theme.space[4],
        alignItems: side === "left" ? "flex-start" : "flex-end",
        // Twice as far from its edge of the screen as the padding of the screen alone.
        ...(side === "left" ? { paddingLeft: theme.space[4] } : { paddingRight: theme.space[4] }),
      }}
    >
      {button ? <CornerButton button={button} /> : null}
    </View>
  );
  return (
    <View
      style={{
        minHeight: HEADER_BUTTON,
        flexDirection: "row",
        alignItems: "center",
        gap: theme.space[3],
      }}
    >
      {corner(left, "left")}
      <View style={{ flexShrink: 1, alignItems: "center" }}>
        <Text variant="heading" numberOfLines={2} testID="header-title">
          {title}
        </Text>
      </View>
      {corner(right, "right")}
    </View>
  );
}
