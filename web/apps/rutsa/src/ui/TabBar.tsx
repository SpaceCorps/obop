import { Text, useTheme } from "@obop/ui-native";
import { Pressable, View } from "react-native";
import { t, type MessageKey } from "../i18n/index.ts";
import { Icon, type IconName } from "./icons.tsx";
import { TABS, type Tab } from "./router.ts";

/** The height of the switcher. */
const BAR = 56;
/** Its distance from the bottom of the screen (above the bar of the system). */
const LIFT = 8;
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
 * the screen, with History, Record (the home screen) and Profile; the tab shown is marked. A
 * placeholder of its look: stage 2 makes it finer.
 */
export function TabBar({
  tab,
  onTab,
  bottom,
}: {
  tab: Tab;
  onTab: (tab: Tab) => void;
  /** The space of the bar of the system under it. */
  bottom: number;
}) {
  const theme = useTheme();
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
        accessibilityRole="tablist"
        style={{
          height: BAR,
          flexDirection: "row",
          padding: 4,
          borderRadius: BAR / 2,
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
                width: 92,
                borderRadius: (BAR - 8) / 2,
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
    </View>
  );
}
