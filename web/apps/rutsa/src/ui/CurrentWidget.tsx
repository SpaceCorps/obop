import { Text, useTheme } from "@obop/ui-native";
import { Pressable, View } from "react-native";
import type { Trip } from "../core/types.ts";
import { t } from "../i18n/index.ts";
import { tripName } from "./trip-label.ts";

/**
 * The current widget: one white card on the home screen, like the row History, always there. It
 * shows the open trip, else the last trip that ended, else "No trips yet.": the trip's name at the
 * top left ("Untitled trip 39" until it is named), a dot and its state at the top right (red
 * Recording, grey Paused, green Ended), the big time without the pauses (running while it
 * records), its km at the bottom left and when it started at the bottom right. A tap opens the
 * trip's screen (D6), where it can be named and, while it records, its count grows.
 */
export function CurrentWidget({
  trip,
  elapsed,
  km,
  started,
  onPress,
}: {
  /** `null` before the first trip. */
  trip: Pick<Trip, "state" | "name" | "number"> | null;
  /** `H:MM:SS` without the pauses. */
  elapsed: string;
  /** Its km: so far while it is open, as Stop counts them. */
  km: string;
  /** `22:42`, or `Yesterday 23:40` for a trip begun before today. */
  started: string;
  onPress: () => void;
}) {
  const theme = useTheme();
  const card = {
    gap: theme.space[1],
    padding: theme.space[4],
    backgroundColor: theme.c.surface,
    borderBottomWidth: 1,
    borderBottomColor: theme.c.border,
  } as const;

  if (trip === null) {
    return (
      <View style={card} testID="current">
        <Text tone="muted" testID="current-none">
          {t("home.none")}
        </Text>
      </View>
    );
  }

  const name = tripName(trip, (number) =>
    number === null ? t("trip.title") : t("trip.untitled", { number }),
  );
  const state =
    trip.state === "recording"
      ? { text: t("home.recording"), color: theme.c.danger.fill }
      : trip.state === "paused"
        ? { text: t("home.paused"), color: theme.c.textMuted }
        : { text: t("home.ended"), color: theme.c.success.fill };
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={[name, state.text, elapsed, km, started].join(", ")}
      accessibilityHint={t("home.current.hint")}
      onPress={onPress}
      android_ripple={{ color: theme.c.accentSoft }}
      testID="current"
      style={({ pressed }) => ({ ...card, opacity: pressed ? 0.85 : 1 })}
    >
      <View style={{ flexDirection: "row", alignItems: "center", gap: theme.space[2] }}>
        <View style={{ flex: 1 }}>
          <Text variant="label" numberOfLines={1} testID="current-name">
            {name}
          </Text>
        </View>
        <View style={{ width: 10, height: 10, borderRadius: 5, backgroundColor: state.color }} />
        <Text tone="muted" testID="current-state">
          {state.text}
        </Text>
      </View>
      <Text
        variant="display"
        accessibilityRole={trip.state === "ended" ? undefined : "timer"}
        testID="elapsed"
      >
        {elapsed}
      </Text>
      <View
        style={{
          flexDirection: "row",
          alignItems: "flex-end",
          justifyContent: "space-between",
          gap: theme.space[3],
        }}
      >
        <Text variant="title" testID="current-km">
          {km}
        </Text>
        <Text tone="muted" testID="current-started">
          {started}
        </Text>
      </View>
    </Pressable>
  );
}
