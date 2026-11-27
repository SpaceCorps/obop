import { useTheme } from "@obop/ui-native";
import { Children, Fragment, type ReactNode } from "react";
import { PixelRatio, View } from "react-native";

/**
 * The rounding of a card of the trip's screen and of Manual add, as the groups of iOS Settings: that
 * of the switcher with the record widget (`TabBar`).
 */
export const GROUP_RADIUS = 28;

/** The distance of the words of a card from its sides: that of the record widget (`RecordWidget`). */
export const GROUP_INSET = 22;

/**
 * The thickness of the line between two rows: a whole number of pixels, so every line is as thick
 * (1 dp is 2.625 pixels on many phones, drawn as 2 or 3 by where it falls).
 */
export const GROUP_LINE = PixelRatio.roundToNearestPixel(1);
const LINE = GROUP_LINE;

/**
 * A group of rows on a rounded card, as the grouped lists of iOS: no line round it, and between two
 * rows a thin line that keeps the padding of the rows at both ends (it does not reach the edges).
 * Rows left out (`null`) take no line.
 */
export function Group({ children, testID }: { children: ReactNode; testID?: string }) {
  const theme = useTheme();
  const rows = Children.toArray(children);
  return (
    <View
      testID={testID}
      style={{
        borderRadius: GROUP_RADIUS,
        overflow: "hidden",
        backgroundColor: theme.c.surface,
      }}
    >
      {rows.map((row, i) => (
        <Fragment key={i}>
          {i > 0 ? (
            <View
              style={{
                height: LINE,
                marginHorizontal: GROUP_INSET,
                backgroundColor: theme.c.border,
              }}
            />
          ) : null}
          {row}
        </Fragment>
      ))}
    </View>
  );
}
