import { describe, expect, test } from "vite-plus/test";
import type { Trip } from "../core/types.ts";
import {
  endedNewestFirst,
  recentLine,
  recentTrips,
  tripLine,
  tripSections,
  type ListWords,
} from "./trips-list.ts";

const STOCKHOLM = "Europe/Stockholm";
const at = (iso: string) => Date.parse(iso);
const MIN = 60_000;
const WORDS: ListWords = {
  today: "Today",
  trips: (count) => (count === 1 ? "1 trip" : `${count} trips`),
  manual: "Added by hand",
};

let next = 0;
const trip = (startIso: string, over: Partial<Trip> = {}): Trip => {
  const startedAt = at(startIso);
  return {
    id: `t${(next += 1)}`,
    state: "ended",
    source: "recorded",
    startedAt,
    endedAt: startedAt + 45 * MIN,
    distanceM: 12_400,
    segment: 1,
    segmentStartedAt: startedAt,
    name: null,
    from: null,
    to: null,
    note: null,
    number: 1,
    ...over,
  };
};

// 2026-11-18 11:00 in Stockholm (UTC+1 in winter).
const NOW = at("2026-11-18T10:00:00Z");

describe("a line of the list", () => {
  test("a trip with nothing typed is called by its times, with its km", () => {
    // 08:30 to 09:15 in Stockholm.
    const line = tripLine(trip("2026-11-18T07:30:00Z"), "en", STOCKHOLM, WORDS);
    expect(line).toMatchObject({
      title: "08:30–09:15",
      subtitle: null,
      km: "12.4 km",
    });
  });

  test("a name is the title, and the times go under it", () => {
    const line = tripLine(
      trip("2026-11-18T07:30:00Z", { name: "Meeting" }),
      "en",
      STOCKHOLM,
      WORDS,
    );
    expect(line).toMatchObject({ title: "Meeting", subtitle: "08:30–09:15" });
  });

  test("without a name, From → To is the title", () => {
    const line = tripLine(
      trip("2026-11-18T07:30:00Z", { from: "Stockholm", to: "Uppsala" }),
      "en",
      STOCKHOLM,
      WORDS,
    );
    expect(line).toMatchObject({ title: "Stockholm → Uppsala", subtitle: "08:30–09:15" });
  });

  test("a trip added by hand is marked (BR-28)", () => {
    const bare = tripLine(
      trip("2026-11-18T07:30:00Z", { source: "manual" }),
      "en",
      STOCKHOLM,
      WORDS,
    );
    expect(bare).toMatchObject({ title: "08:30–09:15", subtitle: "Added by hand" });
    const named = tripLine(
      trip("2026-11-18T07:30:00Z", { source: "manual", name: "Client" }),
      "en",
      STOCKHOLM,
      WORDS,
    );
    expect(named.subtitle).toBe("08:30–09:15 · Added by hand");
  });

  test("a recorded trip has no mark", () => {
    const line = tripLine(
      trip("2026-11-18T07:30:00Z", { name: "Meeting" }),
      "en",
      STOCKHOLM,
      WORDS,
    );
    expect(line.subtitle).not.toContain("Added by hand");
  });

  test("a trip over midnight ends on the clock of the next day", () => {
    // 23:40 to 00:20 in Stockholm.
    const line = tripLine(
      trip("2026-11-17T22:40:00Z", { endedAt: at("2026-11-17T23:20:00Z") }),
      "en",
      STOCKHOLM,
      WORDS,
    );
    expect(line.title).toBe("23:40–00:20");
  });

  test("a trip of 0 km says 0.0 km", () => {
    const line = tripLine(trip("2026-11-18T07:30:00Z", { distanceM: 0 }), "en", STOCKHOLM, WORDS);
    expect(line.km).toBe("0.0 km");
  });
});

describe("the sections of the list", () => {
  test("no trips, no sections", () => {
    expect(tripSections([], NOW, "en", STOCKHOLM, WORDS)).toEqual([]);
  });

  test("days newest first: Today by name, every other day by its date, each with its trips and km", () => {
    const today = trip("2026-11-18T07:30:00Z", { distanceM: 10_000 });
    const todayLater = trip("2026-11-18T09:00:00Z", { distanceM: 2_500 });
    const yesterday = trip("2026-11-17T07:30:00Z");
    const older = trip("2026-11-12T07:30:00Z");
    const sections = tripSections(
      [older, today, yesterday, todayLater],
      NOW,
      "en",
      STOCKHOLM,
      WORDS,
    );
    expect(sections.map((s) => s.header)).toEqual([
      "Today · 2 trips · 12.5 km",
      "Tue, Nov 17 · 1 trip · 12.4 km",
      "Thu, Nov 12 · 1 trip · 12.4 km",
    ]);
    expect(sections[0]?.trips.map((line) => line.id)).toEqual([todayLater.id, today.id]);
  });

  test("a day of another year has its year", () => {
    const sections = tripSections([trip("2025-12-30T07:30:00Z")], NOW, "en", STOCKHOLM, WORDS);
    expect(sections[0]?.header).toBe("Tue, Dec 30, 2025 · 1 trip · 12.4 km");
  });

  test("the open trip is not history yet: it is left out, and so are its day and its km", () => {
    const ended = trip("2026-11-18T07:30:00Z", { distanceM: 3_000 });
    const open = trip("2026-11-18T09:30:00Z", {
      state: "paused",
      endedAt: null,
      distanceM: null,
    });
    const sections = tripSections([ended, open], NOW, "en", STOCKHOLM, WORDS);
    expect(sections.map((s) => s.header)).toEqual(["Today · 1 trip · 3.0 km"]);
    expect(sections[0]?.trips.map((line) => line.id)).toEqual([ended.id]);
    const alone = trip("2026-11-17T09:30:00Z", {
      state: "recording",
      endedAt: null,
      distanceM: null,
    });
    expect(tripSections([alone], NOW, "en", STOCKHOLM, WORDS)).toEqual([]);
  });

  test("a trip belongs to the day of its start in the phone's time zone", () => {
    // 00:30 on 18 November in Stockholm, still the 17th in UTC.
    const sections = tripSections([trip("2026-11-17T23:30:00Z")], NOW, "en", STOCKHOLM, WORDS);
    expect(sections[0]?.key).toBe("2026-11-18");
    expect(sections[0]?.header.startsWith("Today")).toBe(true);
  });
});

describe("the recent trips of the home screen", () => {
  const RECENT = { ...WORDS, yesterday: "Yesterday" };

  test("a name over the day and the times", () => {
    const line = recentLine(
      trip("2026-11-18T07:30:00Z", { name: "Meeting" }),
      NOW,
      "en",
      STOCKHOLM,
      RECENT,
    );
    expect(line).toEqual({
      id: line.id,
      title: "Meeting",
      subtitle: "Today 08:30–09:15",
      km: "12.4 km",
    });
  });

  test("an untitled trip: the times over the day; Yesterday, then the date", () => {
    expect(recentLine(trip("2026-11-17T07:30:00Z"), NOW, "en", STOCKHOLM, RECENT)).toMatchObject({
      title: "08:30–09:15",
      subtitle: "Yesterday",
    });
    expect(recentLine(trip("2026-11-12T07:30:00Z"), NOW, "en", STOCKHOLM, RECENT)).toMatchObject({
      subtitle: "Nov 12",
    });
  });

  test("a trip added by hand is marked", () => {
    const line = recentLine(
      trip("2026-11-18T07:30:00Z", { source: "manual", from: "Office", to: "Client" }),
      NOW,
      "en",
      STOCKHOLM,
      RECENT,
    );
    expect(line).toMatchObject({
      title: "Office → Client",
      subtitle: "Today 08:30–09:15 · Added by hand",
    });
  });

  test("only the trips that ended, newest first", () => {
    const older = trip("2026-11-17T07:30:00Z");
    const newer = trip("2026-11-18T07:30:00Z");
    const open = trip("2026-11-18T09:30:00Z", {
      state: "recording",
      endedAt: null,
      distanceM: null,
    });
    expect(endedNewestFirst([older, open, newer]).map((x) => x.id)).toEqual([newer.id, older.id]);
  });

  test("all the trips of today and yesterday by the phone's day, newest first, none older", () => {
    // 00:30 in Stockholm on the 18th is still the 17th in UTC: it is today.
    const justAfterMidnight = trip("2026-11-17T23:30:00Z");
    const yesterdayMorning = trip("2026-11-17T07:30:00Z");
    // 23:30 in Stockholm on the 16th: the day before yesterday.
    const dayBefore = trip("2026-11-16T22:30:00Z");
    const open = trip("2026-11-18T09:30:00Z", { state: "recording", endedAt: null });
    const many = Array.from({ length: 8 }, (_, i) => trip(`2026-11-18T0${i}:05:00Z`));
    const ids = recentTrips(
      [dayBefore, yesterdayMorning, open, justAfterMidnight, ...many],
      NOW,
      STOCKHOLM,
    ).map((x) => x.id);
    expect(ids).toEqual([
      ...many.map((x) => x.id).reverse(),
      justAfterMidnight.id,
      yesterdayMorning.id,
    ]);
  });
});
