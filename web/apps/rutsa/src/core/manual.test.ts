import { describe, expect, test } from "vite-plus/test";
import {
  checkManual,
  durationFromParts,
  durationText,
  FAST_KMH,
  MAX_KM,
  parseDuration,
  type ManualForm,
} from "./manual.ts";

const STOCKHOLM = "Europe/Stockholm";
const at = (iso: string) => Date.parse(iso);
// 2026-11-25 12:00 in Stockholm (UTC+1 in winter).
const NOW = at("2026-11-25T11:00:00Z");

const FORM: ManualForm = {
  name: "",
  date: "2026-11-24",
  start: "08:30",
  duration: "0:45",
  km: "12.4",
  from: "Office",
  to: "Client",
  note: "",
};
const check = (
  over: Partial<ManualForm>,
  trips: { startedAt: number; endedAt: number | null }[] = [],
) => checkManual({ ...FORM, ...over }, NOW, STOCKHOLM, trips);

describe("a trip added by hand", () => {
  test("a right form gives the trip in UTC, with its km in metres and its texts trimmed", () => {
    const result = check({ name: " Client visit ", note: " Phone was off " });
    expect(result.problems).toEqual({});
    expect(result.nextDay).toBe(false);
    expect(result.trip).toEqual({
      startedAt: at("2026-11-24T07:30:00Z"),
      endedAt: at("2026-11-24T08:15:00Z"),
      distanceM: 12_400,
      name: "Client visit",
      from: "Office",
      to: "Client",
      note: "Phone was off",
    });
  });

  test("the name and the note are optional", () => {
    expect(check({}).trip).toMatchObject({ name: null, note: null });
  });

  test("km with a comma, and times with one digit, a dot or no sign at all", () => {
    const result = check({ km: "12,4", start: "8.30", duration: "0.45" });
    expect(result.trip).toMatchObject({ distanceM: 12_400, startedAt: at("2026-11-24T07:30:00Z") });
    expect(check({ start: "0830", duration: "045" }).trip).toMatchObject({
      startedAt: at("2026-11-24T07:30:00Z"),
      endedAt: at("2026-11-24T08:15:00Z"),
    });
  });

  test("the end is the start and the duration", () => {
    expect(check({ duration: "2:05" }).trip).toMatchObject({
      endedAt: at("2026-11-24T09:35:00Z"),
    });
  });

  test("the times are given once they are times, whatever else is wrong", () => {
    const times = { startedAt: at("2026-11-24T07:30:00Z"), endedAt: at("2026-11-24T08:15:00Z") };
    expect(check({ km: "", from: "" }).times).toEqual(times);
    // Still to come: said under the times, and shown all the same.
    expect(check({ date: "2026-11-25", start: "11:30", duration: "1:00" }).times).toEqual({
      startedAt: at("2026-11-25T10:30:00Z"),
      endedAt: at("2026-11-25T11:30:00Z"),
    });
    expect(check({ duration: "0:00" }).times).toEqual({
      startedAt: times.startedAt,
      endedAt: times.startedAt,
    });
    expect(check({ duration: "" }).times).toBeNull();
    expect(check({ date: "x" }).times).toBeNull();
  });

  test("an end past midnight is on the next day, and says so", () => {
    const result = check({ start: "23:40", duration: "0:40" });
    expect(result.nextDay).toBe(true);
    expect(result.trip).toMatchObject({
      startedAt: at("2026-11-24T22:40:00Z"),
      endedAt: at("2026-11-24T23:20:00Z"),
    });
  });

  test("the duration is the time as it passed, also over a change of the clock", () => {
    // On 2026-10-25 the clock goes from 03:00 back to 02:00 (01:00 UTC). From 01:30 summer time a
    // trip of 2:00 ends at 02:30 winter time: the wall moved one hour, the trip took two.
    const result = check({ date: "2026-10-25", start: "01:30", duration: "2:00" });
    expect(result.trip).toMatchObject({
      startedAt: at("2026-10-24T23:30:00Z"),
      endedAt: at("2026-10-25T01:30:00Z"),
    });
  });

  test("a duration of nothing is refused", () => {
    expect(check({ duration: "0:00" }).problems).toEqual({ duration: "duration_zero" });
  });

  test("a date, a time or a duration that is not one is refused", () => {
    expect(check({ date: "24.11.2026" }).problems.date).toBe("date_format");
    expect(check({ date: "2026-02-30" }).problems.date).toBe("date_format");
    expect(check({ start: "25:00" }).problems.start).toBe("time_format");
    expect(check({ duration: "9:7" }).problems.duration).toBe("duration_format");
    expect(check({ date: "x" }).trip).toBeNull();
  });

  test("nothing in the future: not the day, not the start, not the end", () => {
    expect(check({ date: "2026-11-26" }).problems.date).toBe("date_future");
    // Today, 12:00 now.
    expect(check({ date: "2026-11-25", start: "12:30", duration: "0:30" }).problems.start).toBe(
      "time_future",
    );
    expect(check({ date: "2026-11-25", start: "11:30", duration: "1:00" }).problems).toEqual({
      duration: "end_future",
    });
    // Over midnight into today, past now.
    expect(check({ date: "2026-11-24", start: "23:00", duration: "14:00" }).problems.duration).toBe(
      "end_future",
    );
  });

  test("km over 0 and at most the maximum", () => {
    expect(check({ km: "" }).problems.km).toBe("km_format");
    expect(check({ km: "12 km" }).problems.km).toBe("km_format");
    expect(check({ km: "0" }).problems.km).toBe("km_zero");
    expect(check({ km: String(MAX_KM + 1) }).problems.km).toBe("km_too_far");
    expect(check({ km: String(MAX_KM) }).problems.km).toBeUndefined();
  });

  test("From and To are required; texts too long are refused", () => {
    expect(check({ from: " ", to: "" }).problems).toEqual({ from: "required", to: "required" });
    expect(check({ from: "x".repeat(201) }).problems.from).toBe("too_long");
    expect(check({ name: "x".repeat(101) }).problems.name).toBe("too_long");
    expect(check({ note: "x".repeat(1001) }).problems.note).toBe("too_long");
  });

  test("no overlap with another trip, the open one included until now", () => {
    const other = { startedAt: at("2026-11-24T08:00:00Z"), endedAt: at("2026-11-24T08:30:00Z") };
    // 08:30–09:15 local is 07:30–08:15 UTC: it overlaps 08:00–08:30 UTC.
    expect(check({}, [other]).problems.start).toBe("overlap");
    // Touching is not overlapping: one ends when the other starts.
    const before = { startedAt: at("2026-11-24T07:00:00Z"), endedAt: at("2026-11-24T07:30:00Z") };
    expect(check({}, [before]).problems).toEqual({});
    const open = { startedAt: at("2026-11-25T10:00:00Z"), endedAt: null };
    expect(
      check({ date: "2026-11-25", start: "11:30", duration: "0:15" }, [open]).problems.start,
    ).toBe("overlap");
  });

  test("the average speed is given, so a fast one can be asked about", () => {
    // 150 km in 45 min is 200 km/h.
    const fast = check({ km: "150" });
    expect(fast.speedKmh).toBeCloseTo(200, 5);
    expect((fast.speedKmh ?? 0) > FAST_KMH).toBe(true);
    expect(check({}).speedKmh).toBeCloseTo(12.4 / 0.75, 5);
    expect(check({ km: "x" }).speedKmh).toBeNull();
  });

  test("a duration is kept as hours, minutes and seconds", () => {
    expect(durationText(0)).toBe("0:00:00");
    expect(durationText(4530)).toBe("1:15:30");
    expect(durationText(23 * 3600 + 59 * 60 + 59)).toBe("23:59:59");
  });

  test("a duration is read with its seconds, or without them", () => {
    expect(parseDuration("1:15:30")).toBe(4530);
    expect(parseDuration("0:00:45")).toBe(45);
    expect(parseDuration("1:15")).toBe(4500);
    expect(parseDuration("1:60:00")).toBeNull();
    expect(parseDuration("1:15:60")).toBeNull();
    expect(parseDuration("24:00:00")).toBeNull();
  });

  test("the duration from its three fields: an empty one is 0", () => {
    expect(parseDuration(durationFromParts("1", "15", "30"))).toBe(4530);
    expect(parseDuration(durationFromParts("", "5", ""))).toBe(300);
    expect(parseDuration(durationFromParts("", "", ""))).toBe(0);
    expect(parseDuration(durationFromParts(" 2 ", "", "7"))).toBe(7207);
    // Minutes or seconds over 59, hours over 23, or what is not a number: refused.
    expect(parseDuration(durationFromParts("", "75", ""))).toBeNull();
    expect(parseDuration(durationFromParts("", "", "60"))).toBeNull();
    expect(parseDuration(durationFromParts("24", "", ""))).toBeNull();
    expect(parseDuration(durationFromParts("1.", "", ""))).toBeNull();
  });

  test("the seconds of the duration are in the end", () => {
    expect(check({ duration: "0:45:30" }).trip).toMatchObject({
      endedAt: at("2026-11-24T08:15:30Z"),
    });
    expect(check({ duration: "0:00:30" }).problems).toEqual({});
  });
});
