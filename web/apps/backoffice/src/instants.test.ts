import { describe, expect, it } from "vite-plus/test";
import { dayInZone, fromZonedInput, offsetSeconds, toZonedInput } from "./instants.ts";

const SE = "Europe/Berlin";
const value = (text: string, zone = SE) => {
  const r = fromZonedInput(text, zone);
  if (!("value" in r)) throw new Error(`${text}: ${r.error}`);
  return r.value;
};

describe("instants in the platform zone", () => {
  it("reads the wall clock of the zone and writes the canonical UTC form", () => {
    expect(value("2026-03-29 03:30")).toBe("2026-03-29T01:30:00.000000Z");
    expect(value("2026-03-29T03:30:15")).toBe("2026-03-29T01:30:15.000000Z");
    expect(value("2026-01-15 12:00")).toBe("2026-01-15T11:00:00.000000Z");
    expect(value("2026-01-15 12:00", "UTC")).toBe("2026-01-15T12:00:00.000000Z");
    expect(value("2026-01-15 12:00", "Asia/Tokyo")).toBe("2026-01-15T03:00:00.000000Z");
    expect(value("2026-03-29 03:30:00.123456")).toBe("2026-03-29T01:30:00.123456Z");
  });

  it("takes an explicit offset or Z as the instant itself", () => {
    expect(value("2026-03-29 03:30Z")).toBe("2026-03-29T03:30:00.000000Z");
    expect(value("2026-03-29T03:30:00+02:00")).toBe("2026-03-29T01:30:00.000000Z");
    expect(value("2026-03-29 03:30 -05:30")).toBe("2026-03-29T09:00:00.000000Z");
  });

  it("reads the hour that happens twice as its first occurrence and refuses the one that does not happen", () => {
    // 2026-10-25: clocks go back from 03:00 to 02:00, so 02:30 is 00:30Z and again 01:30Z.
    expect(value("2026-10-25 02:30")).toBe("2026-10-25T00:30:00.000000Z");
    expect(value("2026-10-25 02:30+01:00")).toBe("2026-10-25T01:30:00.000000Z");
    // 2026-03-29: clocks go forward from 02:00 to 03:00, so 02:30 does not exist.
    expect(fromZonedInput("2026-03-29 02:30", SE)).toEqual({ error: "gap" });
    expect(value("2026-03-29 03:00")).toBe("2026-03-29T01:00:00.000000Z");
    expect(value("2026-03-29 01:59:59")).toBe("2026-03-29T00:59:59.000000Z");
  });

  it("refuses what is not an instant", () => {
    for (const bad of [
      "",
      "garbage",
      "2026-03-29",
      "2026-02-30 10:00",
      "2026-13-01 00:00",
      "2026-03-29 24:00",
      "2026-03-29 10:60",
      "2026-03-29 10:00:60",
      "2026-03-29 10:00:00.1234567",
      "2026-03-29 10:00 +25:00",
      "29.03.2026 10:00",
      "0000-01-01 00:00",
    ])
      expect(fromZonedInput(bad, SE), bad).toEqual({ error: "format" });
    expect(fromZonedInput("2026-03-29 10:00", "Mars/Base")).toEqual({ error: "zone" });
    expect(fromZonedInput("0001-01-01 00:00+01:00", SE)).toEqual({ error: "range" });
    expect(fromZonedInput("9999-12-31 23:59:59.999999-01:00", SE)).toEqual({ error: "range" });
  });

  it("takes every instant from the first of 0001 to the last of 9999, as the server does", () => {
    // Date.UTC would read the years 0 to 99 as 1900 to 1999 and the ends would be 1901.
    expect(value("0001-01-01 00:00Z")).toBe("0001-01-01T00:00:00.000000Z");
    expect(value("0099-06-01 12:00", "UTC")).toBe("0099-06-01T12:00:00.000000Z");
    expect(value("0100-06-01 12:00", "UTC")).toBe("0100-06-01T12:00:00.000000Z");
    expect(value("1850-06-01 12:00", "UTC")).toBe("1850-06-01T12:00:00.000000Z");
    expect(value("1900-12-31 23:59Z")).toBe("1900-12-31T23:59:00.000000Z");
    expect(value("9999-12-31 23:59:59.999999Z")).toBe("9999-12-31T23:59:59.999999Z");
  });

  it("shows an instant on the wall clock of the zone and round trips", () => {
    expect(toZonedInput("2026-03-31T22:30:00.000000Z", SE)).toBe("2026-04-01 00:30");
    expect(toZonedInput("2026-10-31T23:30:00.000000Z", SE)).toBe("2026-11-01 00:30");
    expect(toZonedInput("2026-03-29T01:30:00.123456Z", SE)).toBe("2026-03-29 03:30:00.123456");
    expect(toZonedInput("2026-03-29T01:30:07.000000Z", SE)).toBe("2026-03-29 03:30:07");
    expect(toZonedInput(null, SE)).toBe("");
    expect(toZonedInput("not an instant", SE)).toBe("");
    for (const iso of [
      "2026-03-29T01:30:00.000000Z",
      "2026-10-25T00:30:00.000000Z",
      "2026-10-25T01:30:00.000001Z",
      "1999-12-31T23:59:59.999999Z",
      "2026-07-01T12:00:00.500000Z",
    ]) {
      const shown = toZonedInput(iso, SE);
      // Writing back what is shown keeps the instant, except for the repeated autumn hour,
      // which the offset resolves. What is shown must always be accepted.
      const back = fromZonedInput(shown, SE);
      expect("value" in back, `${iso} shown as ${shown}: ${JSON.stringify(back)}`).toBe(true);
      if ("value" in back && !iso.startsWith("2026-10-25T01")) expect(back.value).toBe(iso);
    }
    // Instants before the zone had standard time are shown with the offset it had, to the second,
    // and come back whole.
    for (const iso of [
      "0001-01-01T00:00:00.000000Z",
      "0099-06-01T12:00:00.000000Z",
      "1850-06-01T12:00:00.000000Z",
      "1900-12-31T23:59:00.000000Z",
    ]) {
      const back = fromZonedInput(toZonedInput(iso, SE), SE);
      expect(back, iso).toEqual({ value: iso });
    }
  });

  it("shows the instants at the ends of the range in a form that is accepted again, in any zone", () => {
    // In New York the wall clock of 0001-01-01T00:00Z is in the year 0000, and in Tokyo that of
    // 9999-12-31T23:59:59Z is in the year 10000: those are shown in UTC with a Z.
    for (const zone of ["UTC", SE, "America/New_York", "Asia/Tokyo", "Pacific/Kiritimati"])
      for (const iso of [
        "0001-01-01T00:00:00.000000Z",
        "0001-01-01T00:00:00.000001Z",
        "0001-01-02T00:00:00.000000Z",
        "9999-12-30T23:59:59.999999Z",
        "9999-12-31T23:59:59.999999Z",
      ]) {
        const shown = toZonedInput(iso, zone);
        expect(shown, `${iso} in ${zone}`).not.toBe("");
        expect(fromZonedInput(shown, zone), `${iso} in ${zone} shown as ${shown}`).toEqual({
          value: iso,
        });
      }
    expect(toZonedInput("0001-01-01T00:00:00.000000Z", "America/New_York")).toBe(
      "0001-01-01 00:00Z",
    );
    expect(toZonedInput("0001-01-01T00:00:00.000000Z", "UTC")).toBe("0001-01-01 00:00");
  });

  it("names the date of an instant in the zone", () => {
    expect(dayInZone("2026-03-31T22:30:00.000000Z", SE)).toBe("2026-04-01");
    expect(dayInZone("2026-03-31T22:30:00.000000Z", "UTC")).toBe("2026-03-31");
    expect(dayInZone("2026-10-25T22:30:00.000000Z", SE)).toBe("2026-10-25");
    expect(dayInZone(undefined, SE)).toBe("");
  });

  it("knows the zone offsets", () => {
    expect(offsetSeconds(SE, Date.UTC(2026, 0, 1))).toBe(3600);
    expect(offsetSeconds(SE, Date.UTC(2026, 6, 1))).toBe(7200);
    expect(offsetSeconds("UTC", Date.UTC(2026, 6, 1))).toBe(0);
  });
});
