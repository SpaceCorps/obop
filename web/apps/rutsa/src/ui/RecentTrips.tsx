import { useTheme } from "@obop/ui-native";
import { View } from "react-native";
import { t } from "../i18n/index.ts";
import { Group } from "./Group.tsx";
import { ListRow } from "./ListRow.tsx";
import { SectionHeader } from "./SectionHeader.tsx";
import type { TripLine } from "./trips-list.ts";

/**
 * The recent trips of the home screen: the header "Recent trips" with "Show more" at its right
 * (the main colour), which opens the tab History (`SectionHeader`); under it the trips of today and
 * yesterday (`recentTrips`), all of them, on a rounded card, as in History (`Group`, `ListRow`,
 * each with the arrow). The screen scrolls under the header and the switcher, as History. A tap on
 * a row opens the trip's screen.
 */
export function RecentTrips({
  lines,
  onOpen,
  onHistory,
}: {
  /** The trips of today and yesterday, newest first. */
  lines: readonly TripLine[];
  onOpen: (tripId: string) => void;
  /** "Show more": the tab History. */
  onHistory: () => void;
}) {
  const theme = useTheme();
  return (
    <View style={{ gap: theme.space[2] }} testID="recent">
      <SectionHeader
        title={t("home.recent")}
        action={{ label: t("home.history"), onPress: onHistory, testID: "open-history" }}
      />
      {lines.length === 0 ? null : (
        <Group testID="recent-list">
          {lines.map((line) => (
            <ListRow
              key={line.id}
              title={line.title}
              subtitle={line.subtitle ?? undefined}
              value={line.km}
              onPress={() => onOpen(line.id)}
              testID={`recent-${line.id}`}
            />
          ))}
        </Group>
      )}
    </View>
  );
}
