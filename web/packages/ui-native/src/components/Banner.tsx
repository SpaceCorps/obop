import { useEffect } from "react";
import { AccessibilityInfo, Platform, View } from "react-native";
import { bannerAnnouncement } from "../platform.ts";
import { useTheme, useWords } from "../theme.tsx";
import { Button } from "./Button.tsx";
import { Text } from "./Text.tsx";

export type BannerTone = "info" | "success" | "warning" | "danger";

export type BannerProps = {
  tone: BannerTone;
  title: string;
  /** More words under the title. */
  message?: string;
  action?: { label: string; onPress: () => void };
  testID?: string;
};

// A glyph and the word of the tone go in front of the text, so that no meaning is in colour alone.
const GLYPH: Record<BannerTone, string> = {
  info: "i",
  success: "✓",
  warning: "!",
  danger: "!",
};

/** A message that stays in the page. A warning or a problem is announced as soon as it appears. */
export function Banner({ tone, title, message, action, testID }: BannerProps) {
  const t = useTheme();
  const words = useWords();
  const word = words[tone];
  const sig = t.c[tone];
  const spoken = [`${word}: ${title}`, message].filter(Boolean).join(". ");
  // iOS has no live region: a warning or a problem that appears is announced (platform.ts).
  const announcement = bannerAnnouncement(Platform.OS, tone, spoken);
  useEffect(() => {
    if (announcement !== null) AccessibilityInfo.announceForAccessibility(announcement);
  }, [announcement]);
  return (
    <View
      accessible
      accessibilityRole="alert"
      accessibilityLabel={spoken}
      accessibilityLiveRegion={tone === "danger" || tone === "warning" ? "assertive" : "polite"}
      testID={testID}
      style={{
        flexDirection: "row",
        gap: t.space[3],
        padding: t.space[4],
        backgroundColor: sig.soft,
        borderRadius: t.radius.lg,
        borderLeftWidth: 4,
        borderLeftColor: sig.fill,
      }}
    >
      <View
        style={{
          width: 28,
          height: 28,
          borderRadius: 14,
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: sig.fill,
        }}
      >
        <Text variant="label" color={t.c.surface}>
          {GLYPH[tone]}
        </Text>
      </View>
      <View style={{ flex: 1, gap: t.space[1] }}>
        <Text variant="label" color={sig.ink}>
          {`${word}: ${title}`}
        </Text>
        {message ? <Text color={sig.ink}>{message}</Text> : null}
        {action ? (
          <View style={{ marginTop: t.space[2] }}>
            <Button label={action.label} onPress={action.onPress} variant="secondary" />
          </View>
        ) : null}
      </View>
    </View>
  );
}
