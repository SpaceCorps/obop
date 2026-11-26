import { Text, useTheme } from "@obop/ui-native";
import type { ReactNode, RefObject } from "react";
import { Pressable, View, type HostInstance } from "react-native";
import { t, type MessageKey } from "../i18n/index.ts";
import { Icon, type IconName } from "./icons.tsx";
import { TABS, type Tab } from "./router.ts";

/** The height of the switcher (of its row of tabs). */
const BAR = 56;
/** Its distance from the bottom of the screen (above the bar of the system). */
const LIFT = 14;
/** The padding of the switcher round its tabs. */
const PAD = 4;
/** The width of a tab. */
const TAB_WIDTH = 92;
/** The width of the switcher: its tabs, its padding and its line; the same with the widget in it. */
const WIDTH = TABS.length * TAB_WIDTH + 2 * PAD + 2;
/**
 * The room the tabs leave at their bottom for the switcher, which lies over them: its height,
 * its distance from the bottom and a gap above it.
 */
export const TAB_BAR_ROOM = BAR + LIFT + 4;

const TAB_LOOK: Readonly<Record<Tab, { icon: IconName; label: MessageKey }>> = {
  profile: { icon: "person", label: "tabs.profile" },
  history: { icon: "clock", label: "tabs.history" },
  record: { icon: "record", label: "tabs.record" },
};

/**
 * The switcher at the bottom of the tabs, as in Telegram: a small round-ended bar that lies over
 * the screen, with History, Record (the home screen) and Profile; the tab shown is marked. On
 * Record it holds the record widget over its tabs (`header`): it is then as wide as ever, and
 * higher. A placeholder of its look: stage 2 makes it finer.
 */
export function TabBar({
  tab,
  onTab,
  bottom,
  header,
  barRef,
}: {
  tab: Tab;
  onTab: (tab: Tab) => void;
  /** The space of the bar of the system under it. */
  bottom: number;
  /** Drawn over the tabs, in the switcher: the record widget on Record. */
  header?: ReactNode;
  /** The switcher itself, measured by what ends above it (the recent trips). */
  barRef?: RefObject<HostInstance | null>;
}) {
  const theme = useTheme();
  const wide = header !== undefined;
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
        style={{
          width: WIDTH,
          padding: PAD,
          borderRadius: wide ? 28 : BAR / 2,
          backgroundColor: theme.c.surface,
          borderWidth: 1,
          borderColor: theme.c.border,
          elevation: 8,
          shadowColor: "#000",
          shadowOpacity: 0.15,
          shadowRadius: 12,
          shadowOffset: { width: 0, height: 4 },
        }}
      >
        {wide ? (
          // The record widget over the tabs, as wide as the switcher (its padding taken back, so
          // the widget keeps its own distances from the sides).
          <View
            style={{
              marginHorizontal: -PAD,
              // The round button of the widget as far from the top edge as from the tabs (and
              // from the right edge, in `screens/Home.tsx`).
              paddingTop: theme.space[4] - PAD - 3,
              paddingBottom: theme.space[3] + 3,
            }}
          >
            {header}
          </View>
        ) : null}
        {tabs}
      </View>
    </View>
  );
}
