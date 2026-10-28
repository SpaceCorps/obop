import type { ReactNode } from "react";
import { ScrollView, View } from "react-native";
import { useInsets, useTheme, useWords } from "../theme.tsx";
import { Button } from "./Button.tsx";
import { Text } from "./Text.tsx";

export type ScreenProps = {
  title: string;
  /** Shows a back button in the header. */
  onBack?: () => void;
  /** Actions pinned under the content (the Stop button, Submit). */
  footer?: ReactNode;
  /** `false` only for a placeholder with nothing to scroll (the Loading screens). Every real screen scrolls, the recording screen too, so that large text never hides a banner. */
  scroll?: boolean;
  children: ReactNode;
};

/** The frame of a screen: background, space for the system bars, a title and an optional footer. */
export function Screen({ title, onBack, footer, scroll = true, children }: ScreenProps) {
  const t = useTheme();
  const insets = useInsets();
  const words = useWords();
  const body = <View style={{ gap: t.space[4], padding: t.space[4] }}>{children}</View>;
  return (
    <View
      style={{
        flex: 1,
        backgroundColor: t.c.bg,
        paddingTop: insets.top,
        paddingLeft: insets.left,
        paddingRight: insets.right,
      }}
    >
      <View style={{ paddingHorizontal: t.space[4], paddingTop: t.space[3], gap: t.space[2] }}>
        {onBack ? (
          <View style={{ alignItems: "flex-start" }}>
            <Button label={words.back} variant="ghost" onPress={onBack} />
          </View>
        ) : null}
        <Text variant="title">{title}</Text>
      </View>
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
            backgroundColor: t.c.surface,
            borderTopWidth: 1,
            borderTopColor: t.c.border,
          }}
        >
          {footer}
        </View>
      ) : (
        <View style={{ height: insets.bottom }} />
      )}
    </View>
  );
}
