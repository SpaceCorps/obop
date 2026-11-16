import type { ReactNode } from "react";
import { View } from "react-native";
import { Text } from "./components/Text.tsx";
import { InsetsProvider, ThemeProvider, useTheme } from "./theme.tsx";
import type { ColorMode } from "./tokens.native.ts";

/**
 * Stories draw a component twice, in a phone-wide frame in light mode and in dark mode. The web
 * Storybook shows layout, colour and the component states; it does not show touch, safe areas, the
 * system font or TalkBack, which are checked on a phone (docs/spec/mobile.md 8.7).
 */
function Page({ height, children }: { height: number; children: ReactNode }) {
  const t = useTheme();
  return (
    <View
      style={{
        width: 360,
        height,
        overflow: "hidden",
        borderRadius: 16,
        backgroundColor: t.c.bg,
        borderWidth: 1,
        borderColor: t.c.border,
      }}
    >
      {children}
    </View>
  );
}

export function PhoneFrames({ height = 520, children }: { height?: number; children: ReactNode }) {
  return (
    <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 24, padding: 24 }}>
      {(["light", "dark"] as const satisfies readonly ColorMode[]).map((mode) => (
        <View key={mode} style={{ gap: 8 }}>
          <ThemeProvider mode="light">
            <Text variant="caption">{mode}</Text>
          </ThemeProvider>
          <ThemeProvider mode={mode}>
            <InsetsProvider insets={{ top: 24, right: 0, bottom: 24, left: 0 }}>
              <Page height={height}>{children}</Page>
            </InsetsProvider>
          </ThemeProvider>
        </View>
      ))}
    </View>
  );
}

/** Pads the content of a component story the way a screen body does. */
export function Padded({ children }: { children: ReactNode }) {
  return <View style={{ padding: 16, gap: 12 }}>{children}</View>;
}
