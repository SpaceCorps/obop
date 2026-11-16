import { Text, useTheme } from "@obop/ui-native";
import { Pressable, View } from "react-native";
import { t } from "../i18n/index.ts";

export type HostBarProps = {
  /** The host and the port of the server: `shop.example.com`, `localhost:18180`. */
  host: string;
  /** The address is plain http. */
  notEncrypted: boolean;
  /** The build can speak plain http or is open to DevTools. */
  testBuild: boolean;
  /** The day the address was stored, while that is less than 7 days ago. */
  newSince: string | null;
  /** A tap opens "This app". */
  onPress: () => void;
};

function Chip({ label, tone }: { label: string; tone: "danger" | "warning" | "info" }) {
  const theme = useTheme();
  const signal = theme.c[tone];
  return (
    <View
      style={{
        paddingHorizontal: theme.space[2],
        paddingVertical: 2,
        borderRadius: theme.radius.pill,
        backgroundColor: signal.soft,
        borderWidth: 1,
        borderColor: signal.fill,
      }}
    >
      <Text variant="caption" color={signal.ink}>
        {label}
      </Text>
    </View>
  );
}

/**
 * The strip that names the server. The app draws it above the page at every start
 * and every return, and the page cannot remove it: nothing the page sends hides it, and a tap on
 * it is the way to "This app" that needs no page. What it cannot do is tell a look-alike name
 * from the right one; that stays with the person.
 *
 * Every state is in words (the chips), never in colour alone.
 */
export function HostBar({ host, notEncrypted, testBuild, newSince, onPress }: HostBarProps) {
  const theme = useTheme();
  const chips = [
    notEncrypted ? t("bar.notEncrypted") : null,
    testBuild ? t("bar.testBuild") : null,
    newSince === null ? null : t("bar.newSince", { date: newSince }),
  ].filter((chip): chip is string => chip !== null);
  const spoken = [t("bar.label", { host }), ...chips].join(", ");
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={spoken}
      accessibilityHint={t("bar.hint")}
      onPress={onPress}
      testID="host-bar"
      android_ripple={{ color: theme.c.accentSoft }}
      style={({ pressed }) => ({
        minHeight: theme.control.minTouch,
        flexDirection: "row",
        alignItems: "center",
        gap: theme.space[3],
        paddingHorizontal: theme.space[4],
        paddingVertical: theme.space[2],
        backgroundColor: theme.c.surface,
        borderBottomWidth: 1,
        borderBottomColor: theme.c.borderStrong,
        opacity: pressed ? 0.85 : 1,
      })}
    >
      <View style={{ flex: 1, gap: theme.space[1] }}>
        <Text variant="label" numberOfLines={1} ellipsizeMode="middle" testID="host-bar-host">
          {host}
        </Text>
        {chips.length > 0 ? (
          <View style={{ flexDirection: "row", flexWrap: "wrap", gap: theme.space[1] }}>
            {notEncrypted ? <Chip label={t("bar.notEncrypted")} tone="danger" /> : null}
            {testBuild ? <Chip label={t("bar.testBuild")} tone="warning" /> : null}
            {newSince === null ? null : (
              <Chip label={t("bar.newSince", { date: newSince })} tone="info" />
            )}
          </View>
        ) : null}
      </View>
      <Text variant="label" tone="accent">
        {`${t("bar.open")} ›`}
      </Text>
    </Pressable>
  );
}
