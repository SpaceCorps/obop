import { Row, Text, useInsets, useTheme } from "@obop/ui-native";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { Pressable, useWindowDimensions, View, type HostInstance } from "react-native";
import { t } from "../i18n/index.ts";
import { shareRoom } from "./home-room.ts";
import type { TripLine } from "./trips-list.ts";

/** The rows shown before the room under the list has been measured. */
const FIRST_GUESS = 3;

/**
 * The recent trips of the home screen: a grey header "Recent trips" with "Show more" at its
 * right (the main colour), which opens the tab History; under it the newest trips that ended, as
 * in History, up to `limits.maxRows`, as many as fit down to the bottom of the screen (above the
 * switcher) without scrolling (at least one), and the round button over them (under the current
 * widget) takes the room that is left (`shareRoom`), so the list ends one padding of the screen
 * above the bottom. The room, from the top of the round button to the bottom (the top of the list
 * less the height of the button's row, both measured on the screen in the same layout, so they
 * agree, and the same whatever the size of the button), is measured on the
 * screen (`measureInWindow`) after every render (the clock of the home screen renders it every second
 * while a trip records, every 30 s otherwise) and on every layout of the list, so a warning or a
 * banner that comes or goes changes the rows and the button. A tap on a row opens the trip's
 * screen.
 */
export function RecentTrips({
  lines,
  buttonRowRef,
  onOpen,
  onHistory,
  limits,
  onButton,
}: {
  /** All the trips that ended, newest first; the rows that fit are taken from the start. */
  lines: readonly TripLine[];
  /** The row of the round button (with Restart and Pause), above the list: measured with it. */
  buttonRowRef: RefObject<HostInstance | null>;
  onOpen: (tripId: string) => void;
  /** "Show more": the tab History. */
  onHistory: () => void;
  limits: { maxRows: number; minButton: number; maxButton: number };
  /** The size the round button is given. */
  onButton: (size: number) => void;
}) {
  const theme = useTheme();
  const insets = useInsets();
  const window = useWindowDimensions();
  const listRef = useRef<HostInstance>(null);
  const [rowHeight, setRowHeight] = useState(0);
  const [fit, setFit] = useState(FIRST_GUESS);

  const measure = useCallback(() => {
    const list = listRef.current;
    const buttonRow = buttonRowRef.current;
    if (!list || !buttonRow || rowHeight <= 0) return;
    buttonRow.measureInWindow((_bx, _by, _bw, buttonHeight) => {
      list.measureInWindow((_x, top, width) => {
        // While another screen or tab lies over the home screen it is hidden and measures
        // nothing; the rows and the button stay as they were until it is shown again.
        if (width <= 0 || buttonHeight <= 0) return;
        // The bottom of the screen's content: above the switcher (in the insets of the tabs) and
        // the padding at the bottom of the body. The height of the button as it is drawn, not as
        // it is asked: one that is not drawn yet would make the share swing for ever.
        const bottom = window.height - insets.bottom - theme.space[4];
        const share = shareRoom(bottom - top + buttonHeight, rowHeight, limits);
        setFit(share.rows);
        onButton(share.button);
      });
    });
  }, [rowHeight, theme, limits, onButton, buttonRowRef, insets.bottom, window.height]);

  useEffect(() => {
    measure();
  });

  return (
    <View style={{ gap: theme.space[2] }} testID="recent">
      <View style={{ flexDirection: "row", alignItems: "center" }}>
        <View style={{ flex: 1 }}>
          <Text variant="label" tone="muted" accessibilityRole="header">
            {t("home.recent")}
          </Text>
        </View>
        <Pressable
          accessibilityRole="button"
          onPress={onHistory}
          hitSlop={12}
          testID="open-history"
        >
          <Text variant="label" tone="accent">
            {t("home.history")}
          </Text>
        </Pressable>
      </View>
      <View ref={listRef} onLayout={measure} testID="recent-list">
        {lines.slice(0, fit).map((line, i) => (
          <View
            key={line.id}
            onLayout={
              i === 0
                ? (e) => {
                    const height = e.nativeEvent.layout.height;
                    if (height > 0 && height !== rowHeight) setRowHeight(height);
                  }
                : undefined
            }
          >
            <Row
              title={line.title}
              subtitle={line.subtitle ?? undefined}
              value={line.km}
              onPress={() => onOpen(line.id)}
              testID={`recent-${line.id}`}
            />
          </View>
        ))}
      </View>
    </View>
  );
}
