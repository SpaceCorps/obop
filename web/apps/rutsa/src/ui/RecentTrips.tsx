import { useTheme } from "@obop/ui-native";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { View, type HostInstance } from "react-native";
import { t } from "../i18n/index.ts";
import { Group, GROUP_LINE } from "./Group.tsx";
import { ListRow } from "./ListRow.tsx";
import { SectionHeader } from "./SectionHeader.tsx";
import type { TripLine } from "./trips-list.ts";

/** The rows shown before the room under the list has been measured. */
const FIRST_GUESS = 3;

/**
 * The recent trips of the home screen: the header "Recent trips" with "Show more" at its right
 * (the main colour), which opens the tab History (`SectionHeader`); under it the newest trips that
 * ended on a rounded card, as in History (`Group`, `ListRow`, each with the arrow), up to
 * `maxRows`, as many as fit down to the switcher (with the record widget in it) without scrolling
 * (at least one). The room is measured on the screen (`measureInWindow`) after every render (the
 * clock of the home screen renders it every second while a trip records, every 30 s otherwise) and
 * on every layout of the list, so a warning or a banner that comes or goes changes the rows. A tap
 * on a row opens the trip's screen.
 */
export function RecentTrips({
  lines,
  maxRows,
  footerRef,
  onOpen,
  onHistory,
}: {
  /** All the trips that ended, newest first; the rows that fit are taken from the start. */
  lines: readonly TripLine[];
  /** The most rows shown. */
  maxRows: number;
  /** The switcher with the record widget: the list ends above it. */
  footerRef: RefObject<HostInstance | null>;
  onOpen: (tripId: string) => void;
  /** "Show more": the tab History. */
  onHistory: () => void;
}) {
  const theme = useTheme();
  const listRef = useRef<HostInstance>(null);
  const [rowHeight, setRowHeight] = useState(0);
  const [fit, setFit] = useState(FIRST_GUESS);

  const measure = useCallback(() => {
    const list = listRef.current;
    const footer = footerRef.current;
    if (!list || !footer || rowHeight <= 0) return;
    list.measureInWindow((_x, top, width) => {
      footer.measureInWindow((_fx, footerContentTop, _fw, footerHeight) => {
        // While another screen or tab lies over the home screen it is hidden and measures
        // nothing; the rows stay as they were until it is shown again.
        if (width <= 0 || footerHeight <= 0) return;
        // One padding of the screen between the card and the switcher; a line between two rows.
        const room = footerContentTop - theme.space[4] - top;
        const rows = Math.floor((room + GROUP_LINE) / (rowHeight + GROUP_LINE) + 1e-6);
        setFit(Math.max(1, Math.min(maxRows, rows)));
      });
    });
  }, [footerRef, rowHeight, theme, maxRows]);

  useEffect(() => {
    measure();
  });

  return (
    <View style={{ gap: theme.space[2] }} testID="recent">
      <SectionHeader
        title={t("home.recent")}
        action={{ label: t("home.history"), onPress: onHistory, testID: "open-history" }}
      />
      <View ref={listRef} onLayout={measure} testID="recent-list">
        {lines.length === 0 ? null : (
          <Group>
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
                <ListRow
                  title={line.title}
                  subtitle={line.subtitle ?? undefined}
                  value={line.km}
                  onPress={() => onOpen(line.id)}
                  testID={`recent-${line.id}`}
                />
              </View>
            ))}
          </Group>
        )}
      </View>
    </View>
  );
}
