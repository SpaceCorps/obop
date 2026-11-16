import { useTheme } from "@obop/ui-native";
import { Children, type ReactNode } from "react";
import { useWindowDimensions, View } from "react-native";

/**
 * The buttons pinned at the bottom of a screen. Upright they stand one under the other, each as
 * wide as the screen; with the phone on its side they stand next to each other, so that two
 * buttons do not take half of the little height that is left.
 */
export function ButtonRow({ children }: { children: ReactNode }) {
  const theme = useTheme();
  const { width, height } = useWindowDimensions();
  if (width <= height) return <>{children}</>;
  return (
    <View style={{ flexDirection: "row", gap: theme.space[3] }}>
      {Children.map(children, (child) => (
        <View style={{ flex: 1 }}>{child}</View>
      ))}
    </View>
  );
}
