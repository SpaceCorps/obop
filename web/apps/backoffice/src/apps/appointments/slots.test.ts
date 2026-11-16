import { describe, expect, it } from "vite-plus/test";
import { freeSlots, overlaps } from "./slots.ts";

const b = (start: number, end: number) => ({ id: `${start}`, title: "", start, end });

describe("slots", () => {
  it("offers 08:00 to 17:30 on an empty day", () => {
    const s = freeSlots([], 30);
    expect(s[0]).toEqual({ start: 480, free: true });
    expect(s.at(-1)).toEqual({ start: 1050, free: true });
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
  it("widens the day to a booking outside the hours", () => {
    const s = freeSlots([b(1200, 1260)], 30);
    expect(s.at(-1)!.start).toBe(1230);
  });
  it("treats an empty booking as one minute", () => {
    expect(overlaps(b(600, 600), 600, 630)).toBe(true);
    expect(overlaps(b(600, 660), 660, 690)).toBe(false);
  });
});
