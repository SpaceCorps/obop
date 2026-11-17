import { describe, expect, it } from "vite-plus/test";
import type { Command, FieldMeta, ModelMeta, Row } from "../api.ts";
import {
  calendarSpec,
  clockOf,
  eventOf,
  hhmm,
  isGone,
  movable,
  movePatch,
  newOnDay,
  placeDay,
  visibleHours,
  weekSegments,
  type CalEvent,
} from "./calendarModel.ts";

const f = (name: string, extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind: "text",
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});
const model = (fields: FieldMeta[], actions: Command[] = [], extra: Partial<ModelMeta> = {}) =>
  ({
    name: "m",
    description: "",
    can_create: true,
    can_update: true,
    can_delete: true,
    fields,
    actions,
    ...extra,
  }) as ModelMeta;
const state = f("state", {
  kind: "select",
  options: ["booked", "confirmed", "cancelled"],
});

describe("which fields place a record", () => {
  it("a range of days: start and end", () => {
    const fields = [
      f("employee_id", { kind: "ref", to: "hr_employee" }),
      f("start_on", { kind: "date", required: true }),
      f("end_on", { kind: "date", required: true }),
      state,
    ];
    const s = calendarSpec(model(fields), fields)!;
    expect(s.start.name).toBe("start_on");
    expect(s.end?.name).toBe("end_on");
    expect(s.startMinute).toBeUndefined();
    expect(s.color?.name).toBe("state");
    // A leave made from a day starts and ends that day (its end does not keep today).
    expect(newOnDay(s, "2026-12-03")).toEqual({ start_on: "2026-12-03", end_on: "2026-12-03" });
  });

  it("a day with a start and an end time", () => {
    const fields = [
      f("day", { kind: "date", required: true }),
      f("start_minute", { kind: "number" }),
      f("end_minute", { kind: "number" }),
    ];
    const s = calendarSpec(model(fields), fields)!;
    expect([s.start.name, s.startMinute?.name, s.endMinute?.name]).toEqual([
      "day",
      "start_minute",
      "end_minute",
    ]);
    expect(movable(s)).toBe(true);
    expect(newOnDay(s, "2026-12-03")).toEqual({ day: "2026-12-03" });
  });

  it("a day with a start time and a duration; another date field has no times", () => {
    const fields = [
      f("scheduled_on", { kind: "date" }),
      f("signed_on", { kind: "date" }),
      f("scheduled_start_minute", { kind: "number" }),
      f("planned_minutes", { kind: "number" }),
    ];
    const s = calendarSpec(model(fields), fields)!;
    expect(s.start.name).toBe("scheduled_on");
    expect(s.startMinute?.name).toBe("scheduled_start_minute");
    expect(s.duration?.name).toBe("planned_minutes");
    const other = calendarSpec(model(fields), fields, "signed_on")!;
    expect(other.start.name).toBe("signed_on");
    expect(other.startMinute).toBeUndefined();
  });

  it("the package's ui.calendar wins", () => {
    const fields = [
      f("created_on", { kind: "date", required: true }),
      f("begins", { kind: "date" }),
      f("finishes", { kind: "date" }),
      f("tier", { kind: "select", options: ["a", "b"] }),
    ];
    const s = calendarSpec(
      model(fields, [], { ui: { calendar: { start: "begins", end: "finishes", color: "tier" } } }),
      fields,
    )!;
    expect([s.start.name, s.end?.name, s.color?.name]).toEqual(["begins", "finishes", "tier"]);
  });
});

describe("events", () => {
  const fields = [
    f("day", { kind: "date", required: true }),
    f("start_minute", { kind: "number" }),
    f("planned_minutes", { kind: "number" }),
  ];
  const spec = calendarSpec(model(fields), fields)!;

  it("take their times, a duration, or an hour", () => {
    expect(
      eventOf(spec, { id: "a", day: "2026-11-18", start_minute: 600, planned_minutes: 90 }),
    ).toMatchObject({ start: "2026-11-18", end: "2026-11-18", startMin: 600, endMin: 690 });
    expect(eventOf(spec, { id: "b", day: "2026-11-18", start_minute: 600 })).toMatchObject({
      endMin: 660,
    });
    expect(eventOf(spec, { id: "c", day: "2026-11-18" })?.startMin).toBeUndefined();
    expect(eventOf(spec, { id: "d", day: null })).toBeNull();
  });

  it("an end before the start is the start", () => {
    const r = [f("start_on", { kind: "date" }), f("end_on", { kind: "date" })];
    const s = calendarSpec(model(r), r)!;
    expect(eventOf(s, { id: "x", start_on: "2026-11-10", end_on: "2026-11-01" })?.end).toBe(
      "2026-11-10",
    );
  });

  it("cancelled, refused and discarded records are hidden by default", () => {
    const m = model([state]);
    expect(isGone(m, { id: "1", state: "cancelled" })).toBe(true);
    expect(isGone(m, { id: "2", state: "confirmed" })).toBe(false);
  });

  it("times read as HH:MM, and on screen the way the region writes them", () => {
    expect(hhmm(0)).toBe("00:00");
    expect(hhmm(570)).toBe("09:30");
    expect(hhmm(1440)).toBe("24:00");
    // One clock in every language: an English browser does not get "2:30 PM" here (S5).
    expect(clockOf(870)).toBe("14:30");
    expect(clockOf(1440)).toBe("24:00");
  });

  it("a move writes the date, the start and the end or the duration", () => {
    expect(movePatch(spec, "2026-11-19", 540, 630)).toEqual({
      day: "2026-11-19",
      start_minute: 540,
      planned_minutes: 90,
    });
  });
});

const ev = (
  id: string,
  start: string,
  end = start,
  startMin?: number,
  endMin?: number,
): CalEvent => ({
  row: { id } as Row,
  start,
  end,
  ...(startMin !== undefined ? { startMin, endMin } : {}),
});

describe("a month: bars in lanes", () => {
  it("cuts a range at the week's edges and keeps it on one lane", () => {
    const segs = weekSegments(
      [
        ev("long", "2026-11-12", "2026-11-25"),
        ev("one", "2026-11-17"),
        ev("next", "2026-11-17", "2026-11-18"),
      ],
      "2026-11-15",
    );
    const long = segs.find((s) => s.ev.row.id === "long")!;
    expect([long.from, long.to, long.head, long.tail, long.lane]).toEqual([0, 6, false, false, 0]);
    const next = segs.find((s) => s.ev.row.id === "next")!;
    const one = segs.find((s) => s.ev.row.id === "one")!;
    expect(next.lane).toBe(1);
    expect(one.lane).toBe(2);
    expect(weekSegments([ev("x", "2026-11-01")], "2026-11-15")).toEqual([]);
  });
});

describe("a week: the time grid", () => {
  it("puts overlapping events side by side", () => {
    const { placed, overflow } = placeDay([
      ev("a", "d", "d", 540, 600),
      ev("b", "d", "d", 570, 630),
      ev("c", "d", "d", 660, 720),
    ]);
    const by = Object.fromEntries(placed.map((p) => [p.ev.row.id, [p.col, p.cols]]));
    expect(by).toEqual({ a: [0, 2], b: [1, 2], c: [0, 1] });
    expect(overflow).toEqual([]);
  });

  it("gathers what does not fit into one block", () => {
    const crowd = ["a", "b", "c", "d", "e"].map((id) => ev(id, "d", "d", 600, 660));
    const { placed, overflow } = placeDay(crowd, 3);
    expect(placed).toHaveLength(2);
    expect(placed.every((p) => p.cols === 3)).toBe(true);
    expect(overflow).toHaveLength(1);
    expect(overflow[0]).toMatchObject({ col: 2, cols: 3, from: 600, to: 660 });
    expect(overflow[0]!.events).toHaveLength(3);
  });

  it("opens on the working day, wider when events are earlier or later", () => {
    expect(visibleHours([])).toEqual({ from: 8, to: 18 });
    expect(visibleHours([ev("a", "d", "d", 360, 420), ev("b", "d", "d", 1200, 1260)])).toEqual({
      from: 5,
      to: 22,
    });
  });
});
