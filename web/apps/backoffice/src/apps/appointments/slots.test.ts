import { describe, expect, it } from "vite-plus/test";
import { freeSlots, overlaps } from "./slots.ts";

const b = (start: number, end: number) => ({ id: `${start}`, title: "", start, end });

describe("slots", () => {
  it("offers 08:00 to 17:30 on an empty day", () => {
    const s = freeSlots([], 30);
    expect(s[0]).toEqual({ start: 480, free: true, kind: "free" });
    expect(s.at(-1)).toEqual({ start: 1050, free: true, kind: "free" });
    expect(s).toHaveLength(20);
  });
  it("marks the slots a booking covers, with the length asked", () => {
    const s = freeSlots([b(600, 660)], 60);
    const free = (m: number) => s.find((x) => x.start === m)?.free;
    expect(free(540)).toBe(true); // 09:00 to 10:00 touches 10:00
    expect(free(570)).toBe(false);
    expect(free(600)).toBe(false);
    expect(free(630)).toBe(false);
    expect(free(660)).toBe(true);
  });
  it("tells a slot that is too short from a booked one", () => {
    const s = freeSlots([b(600, 660)], 60);
    const kind = (m: number) => s.find((x) => x.start === m)?.kind;
    expect(kind(570)).toBe("short"); // 09:30 is free, but an hour from there runs into 10:00
    expect(kind(600)).toBe("booked");
    expect(kind(630)).toBe("booked");
    expect(kind(540)).toBe("free");
  });
  it("widens the day to a booking outside the hours", () => {
    const s = freeSlots([b(1200, 1260)], 30);
    expect(s.at(-1)!.start).toBe(1230);
  });
  it("treats an empty booking as one minute", () => {
    expect(overlaps(b(600, 600), 600, 630)).toBe(true);
    expect(overlaps(b(600, 660), 660, 690)).toBe(false);
  });
});
