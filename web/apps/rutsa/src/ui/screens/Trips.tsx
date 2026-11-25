import { Button, Row, Screen, Text, useTheme, useWords } from "@obop/ui-native";
import { View } from "react-native";
import { phoneTimeZone } from "../../core/present.ts";
import type { TripRepository } from "../../core/repository.ts";
import { LANGUAGE, t, tp } from "../../i18n/index.ts";
import { useNow, useTrips } from "../hooks.ts";
import type { Nav } from "../router.ts";
import { tripSections, type ListWords } from "../trips-list.ts";

/**
 * The trips on this phone that ended, called History (D5), by the day of their start, newest
 * first, each day with its number of trips and its km; each trip with its name (else From → To,
 * else its times), its km and the mark of a trip added by hand, and a tap opens its screen. The
 * open trip is not history yet: the home screen shows it. Reachable without location (BR-36, US-18 AC1).
 *
 * After `Trips` of `mileage-app` (`src/ui/screens/Trips.tsx`), by day, without its server.
 *
 * A plain list in the scrolling `Screen`, as there: a `SectionList` inside the `ScrollView` of
 * `Screen` would not virtualise. Several hundred trips render well enough for the prototype.
 */
export function Trips({ store, nav }: { store: TripRepository; nav: Nav }) {
  const theme = useTheme();
  const shared = useWords();
  const { trips, loaded } = useTrips(store);
  // Today follows the clock; a minute is close enough for the names of the days.
  const now = useNow(60_000);
  const words: ListWords = {
    today: t("day.today"),
    trips: (count) => tp("trips.count", count),
    manual: t("trips.manual"),
  };
  const sections = tripSections(trips, now, LANGUAGE, phoneTimeZone(), words);

  return (
    <Screen
      title={t("trips.title")}
      // Back across the footer, as on the screen of the open trip, not at the top.
      footer={
        <Button label={shared.back} variant="secondary" onPress={nav.back} testID="trips-back" />
      }
    >
      {loaded && sections.length === 0 ? (
        <Text tone="muted" testID="trips-empty">
          {t("trips.empty")}
        </Text>
      ) : null}
      {sections.map((section) => (
        <View key={section.key} style={{ gap: theme.space[2] }} testID={`section-${section.key}`}>
          <Text variant="label" tone="muted" accessibilityRole="header">
            {section.header}
          </Text>
          <View>
            {section.trips.map((line) => (
              <Row
                key={line.id}
                title={line.title}
                subtitle={line.subtitle ?? undefined}
                value={line.km}
                onPress={() => nav.push({ name: "trip", id: line.id })}
                testID={`trip-${line.id}`}
              />
            ))}
          </View>
        </View>
      ))}
    </Screen>
  );
}
