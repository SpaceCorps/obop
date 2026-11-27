import { Text, useTheme } from "@obop/ui-native";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { AccessibilityInfo, Animated, Easing, Pressable, View } from "react-native";
import type { Trip } from "../core/types.ts";
import { t } from "../i18n/index.ts";
import { GROUP_INSET } from "./Group.tsx";
import { RECORD_RED } from "./RoundButton.tsx";
import { TAB_BAR_LINE } from "./TabBar.tsx";
import { tripName } from "./trip-label.ts";

/** One fade of the dot of a recording, out or in. */
const PULSE_MS = 1000;

/**
 * The red dot of a recording: it fades out and in again and again (`PULSE_MS` each way), so the
 * recording is seen to go on; it stands still when the phone asks for less motion.
 */
export function PulsingDot({ color }: { color: string }) {
  const opacity = useRef(new Animated.Value(1)).current;
  const [still, setStill] = useState(false);
  useEffect(() => {
    let alive = true;
    AccessibilityInfo.isReduceMotionEnabled().then(
      (reduce) => alive && setStill(reduce),
      () => undefined,
    );
    const sub = AccessibilityInfo.addEventListener("reduceMotionChanged", setStill);
    return () => {
      alive = false;
      sub.remove();
    };
  }, []);
  useEffect(() => {
    if (still) {
      opacity.setValue(1);
      return;
    }
    const step = (toValue: number) =>
      Animated.timing(opacity, {
        toValue,
        duration: PULSE_MS,
        easing: Easing.inOut((progress) => Easing.ease(progress)),
        useNativeDriver: true,
      });
    const pulse = Animated.loop(Animated.sequence([step(0.15), step(1)]));
    pulse.start();
    return () => pulse.stop();
  }, [opacity, still]);
  return (
    <Animated.View
      style={{ width: 10, height: 10, borderRadius: 5, backgroundColor: color, opacity }}
    />
  );
}

/**
 * The record widget, with the buttons of a recording in it, in the switcher at the bottom of the
 * three tabs, over its tabs (`TabBar`): the trip's screen keeps
 * `CurrentWidget`. At the left, as there: a red dot (fading out and in, `PulsingDot`) and
 * "Recording" while the
 * trip records, else the trip's name ("Untitled trip 39" until it is named; typed in place,
 * `nameSlot`), then the big time without the pauses, then its km. At the right, centred on their
 * height, the round button (Start, Stop, Resume) with Restart and Pause (Stop while paused) in a
 * column at its left (`controls`). No clock time: the trip's screen has it. Before the first
 * trip: "No trips yet." and the controls. A tap on the card (not on a button or the name) opens
 * the trip's screen.
 */
export function RecordWidget({
  trip,
  elapsed,
  km,
  onPress,
  nameSlot,
  controls,
}: {
  /** `null` before the first trip. */
  trip: Pick<Trip, "state" | "name" | "number"> | null;
  /** `H:MM:SS` without the pauses. */
  elapsed: string;
  /** Its km: so far while it is open, as Stop counts them. */
  km: string;
  onPress: () => void;
  /** The name typed in place, shown while the trip does not record. */
  nameSlot?: ReactNode;
  /** The round button and the column of Restart and Pause, at the right. */
  controls: ReactNode;
}) {
  const theme = useTheme();
  // No ground of its own: it stands in the switcher, whose ground is that of the cards.
  const card = {
    flexDirection: "row",
    alignItems: "center",
    gap: theme.space[2],
  } as const;

  if (trip === null) {
    return (
      <View style={card} testID="current">
        <View style={{ flex: 1, paddingLeft: GROUP_INSET - TAB_BAR_LINE }}>
          <Text tone="muted" testID="current-none">
            {t("home.none")}
          </Text>
        </View>
        {controls}
      </View>
    );
  }

  const recording = trip.state === "recording";
  const name = tripName(trip, (number) =>
    number === null ? t("trip.title") : t("trip.untitled", { number }),
  );
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={[recording ? t("home.recording") : name, elapsed, km].join(", ")}
      accessibilityHint={t("home.current.hint")}
      onPress={onPress}
      android_ripple={{ color: theme.c.accentSoft }}
      testID="current"
      style={({ pressed }) => ({ ...card, opacity: pressed ? 0.85 : 1 })}
    >
      {/* The three as on the trip's screen: the name (or Recording), the big time, the km. */}
      {/* As far in from the left edge of the switcher (with its line) as on the card of a trip. */}
      <View style={{ flex: 1, gap: theme.space[1], paddingLeft: GROUP_INSET - TAB_BAR_LINE }}>
        {recording ? (
          <View style={{ flexDirection: "row", alignItems: "center", gap: theme.space[2] }}>
            <PulsingDot color={RECORD_RED} />
            <Text variant="label" testID="current-state">
              {t("home.recording")}
            </Text>
          </View>
        ) : (
          // In a row of its own: the name typed in place takes the width (`flex: 1`), which in
          // this column would take its height away.
          <View style={{ flexDirection: "row" }}>
            {nameSlot ?? (
              <Text variant="label" numberOfLines={1} testID="current-name">
                {name}
              </Text>
            )}
          </View>
        )}
        {/* A time too wide for the room left (12:00:00) gets a smaller font, not under the
          buttons. */}
        <Text
          variant="display"
          numberOfLines={1}
          adjustsFontSizeToFit
          minimumFontScale={0.6}
          accessibilityRole={recording ? "timer" : undefined}
          testID="elapsed"
        >
          {elapsed}
        </Text>
        <Text variant="title" testID="current-km">
          {km}
        </Text>
      </View>
      {controls}
    </Pressable>
  );
}
