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
  /** The footer has no ground and no line above it: its buttons stand on the screen's background. */
  footerBare?: boolean;
  children: ReactNode;
};

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
  // up goes out of sight that far below the title, as far as it stood from it at the start.
  const body = (
    <View style={{ gap: t.space[4], paddingHorizontal: t.space[4], paddingBottom: t.space[4] }}>
      {children}
    </View>
  );
  const frame = {
    flex: 1,
    backgroundColor: t.c.bg,
    paddingTop: insets.top,
    paddingLeft: insets.left,
    paddingRight: insets.right,
  } as const;
  const content = (
    <>
      {inline ? (
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
      )}
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
              ? {}
              : { backgroundColor: t.c.surface, borderTopWidth: 1, borderTopColor: t.c.border }),
          }}
          testID="screen-footer"
        >
          {footer}
        </View>
      ) : (
        <View style={{ height: insets.bottom }} />
      )}
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
