import { Text, useTheme } from "@obop/ui-native";
import type { ReactNode } from "react";
import { Pressable, View } from "react-native";
import type { Trip } from "../core/types.ts";
import { t } from "../i18n/index.ts";
import { tripName } from "./trip-label.ts";

/** The yellow dot of a trip being added by hand (New); the theme has no yellow. */
const NEW_DOT = "#f4b400";

/**
 * The current widget: one white card on the home screen, like the row History, always there. It
 * shows the open trip, else the last trip that ended, else "No trips yet.": the trip's name at the
 * top left ("Untitled trip 39" until it is named), a dot and its state at the top right (red
 * Recording, grey Paused, green Ended), the big time without the pauses (running while it
 * records), its km at the bottom left and at the bottom right when it started (open) or ended.
 * A tap opens the trip's screen (D6), which shows the same card in the same place of the screen, with the name
 * typed in place (`nameSlot`) and no tap, over the rest of the trip. The screen Manual add (D7)
 * shows it too, for the trip being typed: the state New, the km typed in place (`kmSlot`), and
 * the duration (`elapsedSlot`) and the start at the bottom right (`clockSlot`) chosen with a tap.
 */
export function CurrentWidget({
  trip,
  elapsed,
  km,
  clock,
  onPress,
  nameSlot,
  kmSlot,
  elapsedSlot,
  clockSlot,
}: {
  /** `null` before the first trip; the state `new` for a trip being added by hand. */
  trip: (Pick<Trip, "name" | "number"> & { state: Trip["state"] | "new" }) | null;
  /** `H:MM:SS` without the pauses. */
  elapsed: string;
  /** Its km: so far while it is open, as Stop counts them. */
  km: string;
  /**
   * When it started while it is open, when it ended once it ended: `22:42`, or `Yesterday 23:40`
   * on another day.
   */
  clock: string;
  /** A tap opens the trip (the home screen); without it the card is not a button (the trip's screen). */
  onPress?: () => void;
  /** Drawn in the place of the name: the name typed in place on the trip's screen. */
  nameSlot?: ReactNode;
  /** Drawn in the place of the km: the km typed in place on the screen Manual add. */
  kmSlot?: ReactNode;
  /** Drawn in the place of the big time: the duration chosen on the screen Manual add. */
  elapsedSlot?: ReactNode;
  /** Drawn in the place of the time at the bottom right: the start chosen on Manual add. */
  clockSlot?: ReactNode;
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
        : trip.state === "new"
          ? { text: t("manual.new"), color: NEW_DOT }
          : { text: t("home.ended"), color: theme.c.success.fill };
  const running = trip.state === "recording" || trip.state === "paused";
  const content = (
    <>
      <View style={{ flexDirection: "row", alignItems: "center", gap: theme.space[2] }}>
        <View style={{ flex: 1 }}>
          {nameSlot ?? (
            <Text variant="label" numberOfLines={1} testID="current-name">
              {name}
            </Text>
          )}
        </View>
        <View style={{ width: 10, height: 10, borderRadius: 5, backgroundColor: state.color }} />
        <Text tone="muted" testID="current-state">
          {state.text}
        </Text>
      </View>
      {elapsedSlot ?? (
        <Text variant="display" accessibilityRole={running ? "timer" : undefined} testID="elapsed">
          {elapsed}
        </Text>
      )}
      <View
        style={{
          flexDirection: "row",
          alignItems: "flex-end",
          justifyContent: "space-between",
          gap: theme.space[3],
        }}
      >
        {kmSlot ?? (
          <Text variant="title" testID="current-km">
            {km}
          </Text>
        )}
        {clockSlot ?? (
          <Text tone="muted" testID="current-clock">
            {clock}
          </Text>
        )}
      </View>
    </>
  );
  if (!onPress) {
    return (
      <View style={card} testID="current">
        {content}
      </View>
    );
  }
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={[name, state.text, elapsed, km, clock].join(", ")}
      accessibilityHint={t("home.current.hint")}
      onPress={onPress}
      android_ripple={{ color: theme.c.accentSoft }}
      testID="current"
      style={({ pressed }) => ({ ...card, opacity: pressed ? 0.85 : 1 })}
    >
      {content}
    </Pressable>
  );
}
