import { Text, useTheme } from "@obop/ui-native";
import type { ReactNode, RefObject } from "react";
import { Pressable, View, type HostInstance } from "react-native";
import { t, type MessageKey } from "../i18n/index.ts";
import { Icon, type IconName } from "./icons.tsx";
import { TABS, type Tab } from "./router.ts";

/** The height of the row of tabs, with the padding of the switcher round it. */
const BAR = 56;
/** Its distance from the bottom of the screen (above the bar of the system). */
const LIFT = 14;
/** The padding of the switcher round its tabs. */
const PAD = 4;
/** The width of a tab. */
const TAB_WIDTH = 92;
/** The width of the switcher: its tabs, its padding and its line. */
const WIDTH = TABS.length * TAB_WIDTH + 2 * PAD + 2;
/** The thickness of its line. */
export const TAB_BAR_LINE = 1;
const LINE = TAB_BAR_LINE;
/**
 * The distance of the words of the record widget from the top edge of the switcher (with its line):
 * that of the words of the card of a trip from its top (`CurrentWidget`).
 */
export const WIDGET_TOP = 16;

/**
 * The room the tabs leave at their bottom for the switcher before it has been measured: its row of
 * tabs, its distance from the bottom and a gap above it. Once measured, `onRoom` says the room.
 */
export const TAB_BAR_ROOM = BAR + LIFT + 4;

const TAB_LOOK: Readonly<Record<Tab, { icon: IconName; label: MessageKey }>> = {
  profile: { icon: "person", label: "tabs.profile" },
  history: { icon: "clock", label: "tabs.history" },
  record: { icon: "record", label: "tabs.record" },
};

/**
 * The switcher at the bottom of the tabs, as in Telegram: a round-ended panel that lies over the
 * screen, with the record widget (`header`) over History, Record (the home screen) and Profile; the
 * tab shown is marked. It is at the bottom of every screen, the footer of the app: on a screen over
 * the tabs no tab is marked. A placeholder of its look: stage 2 makes it finer.
 */
export function TabBar({
  tab,
  onTab,
  bottom,
  header,
  barRef,
  onRoom,
}: {
  /** The tab shown, marked; `null` on a screen over the tabs (a trip, Manual add), none marked. */
  tab: Tab | null;
  onTab: (tab: Tab) => void;
  /** The space of the bar of the system under it. */
  bottom: number;
  /** Drawn over the tabs, in the switcher: the record widget. */
  header: ReactNode;
  /** The switcher itself, measured by what ends above it (the recent trips). */
  barRef?: RefObject<HostInstance | null>;
  /** The room the tabs keep at their bottom for it, said each time its height changes. */
  onRoom?: (room: number) => void;
}) {
  const theme = useTheme();
  const tabs = (
    <View style={{ height: BAR - 2 * PAD, flexDirection: "row", justifyContent: "center" }}>
      {TABS.map((name) => {
        const shown = name === tab;
        const look = TAB_LOOK[name];
        const ink = shown ? theme.c.accentInk : theme.c.textMuted;
        return (
          <Pressable
            key={name}
            accessibilityRole="tab"
            accessibilityState={{ selected: shown }}
            accessibilityLabel={t(look.label)}
            onPress={() => onTab(name)}
            testID={`tab-${name}`}
            style={{
              width: TAB_WIDTH,
              borderRadius: (BAR - 2 * PAD) / 2,
              alignItems: "center",
              justifyContent: "center",
              gap: 2,
              backgroundColor: shown ? theme.c.accentSoft : "transparent",
            }}
          >
            <Icon name={look.icon} color={ink} size={20} />
            <Text variant="caption" color={ink}>
              {t(look.label)}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
  return (
    <View
      pointerEvents="box-none"
      style={{
        position: "absolute",
        left: 0,
        right: 0,
        bottom: bottom + LIFT,
        alignItems: "center",
      }}
    >
      <View
        ref={barRef}
        accessibilityRole="tablist"
        testID="tab-bar"
        onLayout={(e) => onRoom?.(e.nativeEvent.layout.height + LIFT + 4)}
        style={{
          width: WIDTH,
          padding: PAD,
          borderRadius: 28,
          backgroundColor: theme.c.surface,
          borderWidth: LINE,
          borderColor: theme.c.border,
          elevation: 8,
          shadowColor: "#000",
          shadowOpacity: 0.15,
          shadowRadius: 12,
          shadowOffset: { width: 0, height: 4 },
        }}
      >
        {/* The record widget over the tabs, as wide as the switcher (its padding taken back): its
          words as far from the left and the top edge as those of the card of a trip
          (`CurrentWidget`); the round button as far from the right edge as from the tabs (in
          `RecordPanel`). */}
        <View
          style={{
            marginHorizontal: -PAD,
            marginTop: -PAD,
            paddingTop: WIDGET_TOP - LINE,
            paddingBottom: theme.space[3] + 3,
          }}
        >
          {header}
        </View>
        {tabs}
      </View>
    </View>
  );
}
