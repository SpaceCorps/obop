import { describe, expect, it } from "vite-plus/test";
import type { FieldMeta, ModelMeta, Row } from "../api.ts";
import {
  buildGrid,
  cellEditable,
  daySumText,
  cellPlan,
  parseCellHours,
  gridSpecOf,
  submittable,
  weekDays,
  weekStart,
  type GridSpec,
} from "./gridModel.ts";

const f = (name: string, extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind: "text",
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});

const fields = [
  f("task_id", { kind: "ref", to: "projects_task", required: true }),
  f("worker", { required: true }),
  f("worked_on", { kind: "date", required: true }),
  f("minutes", { kind: "number", required: true, ui: { display: "duration_minutes" } }),
  f("state", {
    kind: "select",
    required: true,
    governed: true,
    options: ["draft", "submitted", "approved"],
  }),
];
const model = {
  name: "timesheets_entry",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields,
  actions: [],
  access: { own: "worker" },
  ui: { calendar: { start: "worked_on" } },
} as unknown as ModelMeta;

const spec = gridSpecOf(model, fields) as GridSpec;
const entry = (id: string, task: string, day: string, minutes: number, extra: Partial<Row> = {}) =>
  ({
    id,
    task_id: task,
    worker: "ana",
    worked_on: day,
    minutes,
    state: "draft",
    ...extra,
  }) as Row;

describe("the grid of a model", () => {
  it("takes the ref as the line, the date as the day and the duration as the cell", () => {
    expect(spec.line.name).toBe("task_id");
    expect(spec.day.name).toBe("worked_on");
    expect(spec.value.name).toBe("minutes");
    expect(spec.owner?.name).toBe("worker");
    expect(spec.state?.name).toBe("state");
  });

  it("is not offered without a number shown as a duration", () => {
    const plain = fields.map((x) => (x.name === "minutes" ? f("quantity", { kind: "number" }) : x));
    expect(gridSpecOf({ ...model, fields: plain }, plain)).toBeNull();
  });
});

describe("weeks", () => {
  it("start on Monday, also from a Sunday and across a month", () => {
    expect(weekStart("2026-11-16")).toBe("2026-11-16");
    expect(weekStart("2026-11-15")).toBe("2026-11-09");
    expect(weekStart("2026-12-02")).toBe("2026-11-30");
    expect(weekDays("2026-11-30")).toEqual([
      "2026-11-30",
      "2026-12-01",
      "2026-12-02",
      "2026-12-03",
      "2026-12-04",
      "2026-12-05",
      "2026-12-06",
    ]);
  });
});

describe("lines and totals", () => {
  const days = weekDays("2026-11-09");
  const rows = [
    entry("a", "t1", "2026-11-09", 90),
    entry("b", "t1", "2026-11-10", 30),
    entry("c", "t2", "2026-11-10", 60, { worker: "bo" }),
    entry("x", "t1", "2026-11-16", 600),
  ];
  const g = buildGrid(rows, spec, days, [{ ref: "t3", owner: "ana" }]);

  it("has one line per task and person, and leaves out other weeks", () => {
    expect(g.lines.map((l) => l.key)).toEqual(["t1|ana", "t2|bo", "t3|ana"]);
    expect(g.lines[0]!.total).toBe(120);
    expect(g.dayTotals["2026-11-10"]).toBe(90);
    expect(g.total).toBe(180);
  });

  it("submits only the drafts", () => {
    const mixed = buildGrid(
      [...rows, entry("d", "t2", "2026-11-11", 15, { state: "submitted" })],
      spec,
      days,
    );
    expect(submittable(mixed, spec).map((r) => r.id)).toEqual(["a", "b", "c"]);
  });
});

describe("typing into a cell", () => {
  const one = [entry("a", "t1", "2026-11-09", 90)];

  it("creates, changes or deletes the one entry of the cell", () => {
    expect(cellPlan([], 60, spec)).toEqual({ kind: "create", minutes: 60 });
    expect(cellPlan([], null, spec)).toEqual({ kind: "none" });
    expect(cellPlan(one, 120, spec)).toEqual({ kind: "update", id: "a", minutes: 120 });
    expect(cellPlan(one, 90, spec)).toEqual({ kind: "none" });
    expect(cellPlan(one, null, spec)).toEqual({ kind: "delete", id: "a" });
    expect(cellPlan(one, 0, spec)).toEqual({ kind: "delete", id: "a" });
  });

  it("never types over several entries or one that is no longer a draft", () => {
    const two = [...one, entry("b", "t1", "2026-11-09", 30)];
    const sent = [entry("a", "t1", "2026-11-09", 90, { state: "submitted" })];
    expect(cellEditable(two, spec)).toBe(false);
    expect(cellEditable(sent, spec)).toBe(false);
    expect(cellPlan(two, 60, spec)).toEqual({ kind: "locked" });
    expect(cellPlan(sent, 60, spec)).toEqual({ kind: "locked" });
  });
});

describe("reading what is typed into a cell", () => {
  it("reads a bare number as hours, and the other forms as everywhere else", () => {
    expect(parseCellHours("8")).toBe(480);
    expect(parseCellHours("1.5")).toBe(90);
    expect(parseCellHours("1,5")).toBe(90);
    expect(parseCellHours(".5")).toBe(30);
    expect(parseCellHours("1:30")).toBe(90);
    expect(parseCellHours("2.5h")).toBe(150);
    expect(parseCellHours("90m")).toBe(90);
    expect(parseCellHours("90 min")).toBe(90);
    expect(parseCellHours("eight")).toBeNull();
  });
});

describe("daySumText", () => {
  it("writes a day total as hours and minutes on a clock", () => {
    expect(daySumText(450)).toBe("7:30");
    expect(daySumText(480)).toBe("8:00");
    expect(daySumText(5)).toBe("0:05");
    expect(daySumText(0)).toBe("");
  });
});
