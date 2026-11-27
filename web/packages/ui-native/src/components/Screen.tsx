import type { ReactNode } from "react";
import {
  KeyboardAvoidingView,
  Platform,
  ScrollView,
  useWindowDimensions,
  View,
} from "react-native";
import { screenAvoidsKeyboard } from "../platform.ts";
import { useInsets, useTheme, useWords } from "../theme.tsx";
import { Button } from "./Button.tsx";
import { Text } from "./Text.tsx";

export type ScreenProps = {
  title: string;
  /** Drawn in the place of the title text, for a title that can be changed in place; `title` stays its name for the rest. */
  titleSlot?: ReactNode;
  /** Shows a back button in the header. */
  onBack?: () => void;
  /** Actions pinned under the content (the Stop button, Submit). */
  footer?: ReactNode;
  /** `false` only for a placeholder with nothing to scroll (the Loading screens). Every real screen scrolls, the recording screen too, so that large text never hides a banner. */
  scroll?: boolean;
  /** The back button stands at the start of the header, as wide as its word (the default); `false` puts it across the header, its word in the middle. */
  backAtStart?: boolean;
  /** With the phone on its side, the back button and the title share one line, so the content gets more of the little height. */
  compactInLandscape?: boolean;
  /** The footer's ground reaches the left and right edges of the screen, under the side insets; its buttons stay inside them. */
  footerToEdges?: boolean;
  /**
   * The footer has no ground, no line above it and no padding at its top: its buttons stand on the
   * screen's background, the padding at the bottom of the content apart from them.
   */
  footerBare?: boolean;
  /**
   * The header lies over the content, with no ground of its own, and the content scrolls under it
   * and under the system bars, as on iOS: the whole screen is the content's. The number is the
   * height of the title or of `titleSlot` (as the content cannot measure the header), and the
   * content starts that far down; it ends the bottom inset up (under a switcher that lies over it,
   * when the inset holds its room).
   */
  floatingHeaderHeight?: number;
  children: ReactNode;
};

/** The bands of a `Fade`: thin enough that their steps are not seen. */
export const FADE_BANDS = 32;

/** How opaque a `Fade` still is at its far edge: a quarter, so what lies under it stays dimmed. */
export const FADE_LEAST = 0.25;

/**
 * A ground of `color` that fills its parent, whole at one edge and fading to a quarter
 * (`FADE_LEAST`) toward the other: toward the bottom for a header, toward the top for a footer. It
 * is made of plain bands of the colour, each less opaque than the one before (no gradient, which
 * some phones and the emulator draw with coloured streaks), and takes no touch.
 */
export function Fade({ color, toward }: { color: string; toward: "bottom" | "top" }) {
  const bands = Array.from(
    { length: FADE_BANDS },
    (_, i) => 1 - ((1 - FADE_LEAST) * (i + 0.5)) / FADE_BANDS,
  );
  return (
    <View
      pointerEvents="none"
      style={{
        position: "absolute",
        top: 0,
        bottom: 0,
        left: 0,
        right: 0,
        flexDirection: toward === "bottom" ? "column" : "column-reverse",
      }}
      testID="fade"
    >
      {bands.map((opacity, i) => (
        <View key={i} style={{ flex: 1, backgroundColor: color, opacity }} />
      ))}
    </View>
  );
}

/** The frame of a screen: background, space for the system bars, a title and an optional footer. */
export function Screen({
  title,
  titleSlot,
  onBack,
  footer,
  scroll = true,
  backAtStart = true,
  compactInLandscape = false,
  footerToEdges = false,
  footerBare = false,
  floatingHeaderHeight,
  children,
}: ScreenProps) {
  const t = useTheme();
  const insets = useInsets();
  const words = useWords();
  const window = useWindowDimensions();
  const inline = compactInLandscape && window.width > window.height;
  const back = onBack ? (
    <Button label={words.back} variant="ghost" onPress={onBack} fit={backAtStart || inline} />
  ) : null;
  // The space under the title belongs to the header, which does not scroll: content that scrolls
  // up goes out of sight that far below the title, as far as it stood from it at the start. A
  // floating header lies over the content instead, which starts as far down (`floatingHeaderHeight`).
  const floating = floatingHeaderHeight !== undefined;
  const headerTop = inline ? t.space[2] : t.space[3];
  const body = (
    <View
      style={{
        gap: t.space[4],
        paddingHorizontal: t.space[4],
        paddingBottom: t.space[4] + (floating && !footer ? insets.bottom : 0),
        ...(floating
          ? { paddingTop: insets.top + headerTop + floatingHeaderHeight + t.space[4] }
          : {}),
      }}
      testID="screen-body"
    >
      {children}
    </View>
  );
  const frame = {
    flex: 1,
    backgroundColor: t.c.bg,
    paddingTop: floating ? 0 : insets.top,
    paddingLeft: insets.left,
    paddingRight: insets.right,
  } as const;
  const header = inline ? (
    <View
      style={{
        flexDirection: "row",
        alignItems: "center",
        paddingHorizontal: t.space[4],
        paddingTop: t.space[2],
        paddingBottom: t.space[4],
        gap: t.space[3],
      }}
    >
      {back}
      <View style={{ flexShrink: 1 }}>{titleSlot ?? <Text variant="title">{title}</Text>}</View>
    </View>
  ) : (
    <View
      style={{
        paddingHorizontal: t.space[4],
        paddingTop: t.space[3],
        paddingBottom: t.space[4],
        gap: t.space[2],
      }}
    >
      {back === null ? null : <View style={{ alignItems: "flex-start" }}>{back}</View>}
      {titleSlot ?? <Text variant="title">{title}</Text>}
    </View>
  );
  const content = (
    <>
      {floating ? null : header}
      {scroll ? (
        <ScrollView style={{ flex: 1 }} keyboardShouldPersistTaps="handled">
          {body}
        </ScrollView>
      ) : (
        <View style={{ flex: 1 }}>{body}</View>
      )}
      {footer ? (
        <View
          style={{
            gap: t.space[3],
            padding: t.space[4],
            paddingBottom: t.space[4] + insets.bottom,
            ...(footerToEdges
              ? {
                  marginLeft: -insets.left,
                  marginRight: -insets.right,
                  paddingLeft: t.space[4] + insets.left,
                  paddingRight: t.space[4] + insets.right,
                }
              : {}),
            ...(footerBare
              ? { paddingTop: 0 }
              : { backgroundColor: t.c.surface, borderTopWidth: 1, borderTopColor: t.c.border }),
          }}
          testID="screen-footer"
        >
          {footer}
        </View>
      ) : floating ? null : (
        <View style={{ height: insets.bottom }} />
      )}
      {/* Last, so it is drawn over the content that scrolls under it, on a ground of the screen's
        colour that fades from whole at the top to nothing at its bottom. */}
      {floating ? (
        <View
          style={{
            position: "absolute",
            top: 0,
            left: insets.left,
            right: insets.right,
            paddingTop: insets.top,
          }}
          testID="screen-header"
        >
          <Fade color={t.c.bg} toward="bottom" />
          {header}
        </View>
      ) : null}
    </>
  );
  // iOS does not resize the window for the keyboard (Android does): the whole screen pads itself, so
  // that the field and the footer button stay above the keyboard.
  return screenAvoidsKeyboard(Platform.OS) ? (
    <KeyboardAvoidingView behavior="padding" style={frame}>
      {content}
    </KeyboardAvoidingView>
  ) : (
    <View style={frame}>{content}</View>
  );
}
