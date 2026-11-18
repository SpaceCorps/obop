import { describe, expect, it } from "vite-plus/test";
import type { FieldMeta, ModelMeta, Row } from "../api.ts";
import { englishPlural, fieldTitle, keepAcronyms, modelPlural, modelTitle } from "../labels.ts";
import {
  basisPointsText,
  clockText,
  dateShape,
  durationClock,
  durationText,
  kmText,
  linkOf,
  parseBasisPoints,
  parseClock,
  parseDuration,
  parseKm,
  parseRegionDate,
  regionDate,
  titleFromFileName,
  urlSlug,
} from "./display.ts";
import { fillValues, fillsFor, meets, parseRefFilter, refConditions } from "./form.ts";
import { readZoned } from "./DateInput.tsx";

const field = (name: string, extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind: "text",
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});
const model = (name: string, fields: FieldMeta[]): ModelMeta =>
  ({ name, fields, actions: [], can_create: true, can_update: true, can_delete: true }) as never;

describe("durations", () => {
  it("are typed as 1:30, 1.5h or 90 and stored as minutes", () => {
    expect(parseDuration("1:30")).toBe(90);
    expect(parseDuration("1.5h")).toBe(90);
    expect(parseDuration("1,5 h")).toBe(90);
    expect(parseDuration("90")).toBe(90);
    expect(parseDuration("90 min")).toBe(90);
    expect(parseDuration("1h30")).toBe(90);
    expect(parseDuration("1 h 30 min")).toBe(90);
    expect(parseDuration("2 hours")).toBe(120);
    expect(parseDuration("")).toBeNull();
    expect(parseDuration("1:75")).toBeNull();
    expect(parseDuration("soon")).toBeNull();
  });
  it("read as a person says them", () => {
    expect(durationText(45)).toBe("45 min");
    expect(durationText(90)).toBe("1 h 30");
    expect(durationText(3720)).toBe("62 h");
    expect(durationText("")).toBe("");
    expect(durationClock(90)).toBe("1:30");
    expect(durationClock(5)).toBe("0:05");
  });
});

describe("times of day", () => {
  it("are minutes after midnight", () => {
    expect(clockText(870)).toBe("14:30");
    expect(clockText(0)).toBe("00:00");
    expect(parseClock("14:30")).toBe(870);
    expect(parseClock("1430")).toBe(870);
    // A German phone's number keypad has a comma and no colon.
    expect(parseClock("14,30")).toBe(870);
    expect(parseClock("14.30")).toBe(870);
    expect(parseClock("9")).toBe(540);
    expect(parseClock("24:00")).toBeNull();
  });
});

describe("percentages, distances, links", () => {
  it("keep basis points", () => {
    expect(parseBasisPoints("19")).toBe(1900);
    expect(parseBasisPoints("19.5 %")).toBe(1950);
    expect(parseBasisPoints("x")).toBeNull();
    expect(basisPointsText(1900)).toBe("19%");
  });
  it("keep metres while kilometres are typed", () => {
    expect(kmText(200150)).toBe("200.2 km");
    expect(parseKm("0.2", 2)).toBe("200.00");
    expect(parseKm("12.34567", 2)).toBe("12345.67");
    expect(parseKm("3 km", null)).toBe("3000");
  });
  it("point at a phone, a mailbox, a page or a map", () => {
    expect(linkOf("tel", "+49 40 123 45")).toBe("tel:+494012345");
    expect(linkOf("email", "a@b.de")).toBe("mailto:a@b.de");
    expect(linkOf("email", "not mail")).toBeNull();
    expect(linkOf("url", "example.com/x")).toBe("https://example.com/x");
    expect(linkOf("url", "javascript:alert(1)")).toBeNull();
    expect(linkOf("address", "Hafenstr. 1\n20457 Hamburg")).toContain(
      encodeURIComponent("Hafenstr. 1, 20457 Hamburg"),
    );
  });
});

describe("dates in the region's order", () => {
  it("read and parse day.month.year in Germany", () => {
    expect(dateShape("de-DE")).toEqual({ order: ["d", "m", "y"], sep: "." });
    expect(regionDate("2026-11-25", "de-DE")).toBe("25.11.2026");
    expect(parseRegionDate("25.11.2026", new Date(2026, 0, 1), "de-DE")).toBe("2026-11-25");
    expect(parseRegionDate("25.11.26", new Date(2026, 0, 1), "de-DE")).toBe("2026-11-25");
    expect(parseRegionDate("25.11", new Date(2026, 0, 1), "de-DE")).toBe("2026-11-25");
    expect(parseRegionDate("31.02.2026", new Date(2026, 0, 1), "de-DE")).toBeNull();
  });
  it("read month first in the US, and ISO everywhere", () => {
    expect(regionDate("2026-11-25", "en-US")).toBe("11/25/2026");
    expect(parseRegionDate("11/25/2026", new Date(2026, 0, 1), "en-US")).toBe("2026-11-25");
    expect(parseRegionDate("2026-11-25", new Date(2026, 0, 1), "en-US")).toBe("2026-11-25");
    expect(parseRegionDate("20261125", new Date(2026, 0, 1), "en-US")).toBe("2026-11-25");
  });
  it("takes an instant as the zone's wall clock or with an offset", () => {
    expect(readZoned("2026-03-29 03:30+02:00", "UTC")).toBe("2026-03-29 03:30+02:00");
    expect(readZoned("29 March", "UTC")).toBeNull();
  });
});

describe("names", () => {
  it("make slugs and titles", () => {
    expect(urlSlug("Über uns & Kontakt")).toBe("uber-uns-kontakt");
    expect(urlSlug("Straße 7")).toBe("strasse-7");
    expect(titleFromFileName("Quarterly report.final.pdf")).toBe("Quarterly report.final");
    expect(titleFromFileName("scan_0042.jpeg")).toBe("scan 0042");
  });
  it("keep acronyms in capitals", () => {
    expect(keepAcronyms("Seo title")).toBe("SEO title");
    expect(keepAcronyms("Bank iban")).toBe("Bank IBAN");
    expect(fieldTitle(field("vin"))).toBe("VIN");
    expect(fieldTitle(field("sku"))).toBe("SKU");
    expect(fieldTitle(field("national_id"))).toBe("National ID");
    expect(fieldTitle(field("distance_m", { kind: "decimal" }))).toBe("Distance");
    expect(modelTitle({ name: "helpdesk_sla" }, "helpdesk")).toBe("SLA");
    expect(fieldTitle(field("start_minute", { kind: "number" }))).toBe("Start time");
    expect(fieldTitle(field("resolution_minutes", { kind: "number" }))).toBe("Resolution time");
    expect(fieldTitle(field("minutes", { kind: "number" }))).toBe("Duration");
  });
  it("prefer the server's own label, English included", () => {
    expect(fieldTitle(field("seo_title", { labels: { en: "Search title" } }))).toBe("Search title");
  });
  it("make English plurals, or take the server's", () => {
    expect(englishPlural("Entry")).toBe("Entries");
    expect(englishPlural("Tax")).toBe("Taxes");
    expect(englishPlural("BOM")).toBe("BOMs");
    expect(englishPlural("Bank statement line")).toBe("Bank statement lines");
    expect(modelPlural({ name: "accounting_move", plural: "Journal entries" }, "accounting")).toBe(
      "Journal entries",
    );
    expect(modelPlural({ name: "manufacturing_bom" }, "manufacturing")).toBe("BOMs");
  });
});

describe("picking a record fills what follows it", () => {
  const product = model("product", [
    field("name", { required: true }),
    field("list_price_minor", { kind: "money" }),
    field("cost_minor", { kind: "money" }),
  ]);
  const line = model("sale_order_line", [
    field("order_id", { kind: "ref", to: "sale_order", required: true }),
    field("product_id", { kind: "ref", to: "product" }),
    field("description"),
    field("unit_price_minor", { kind: "money" }),
  ]);
  const ref = line.fields[1]!;
  const asEdit = (_f: FieldMeta, v: unknown) => String(v);
  const drill = { id: "p1", name: "Drill", list_price_minor: 12900, cost_minor: 8000 } as Row;
  const saw = { id: "p2", name: "Saw", list_price_minor: 4500 } as Row;

  it("a line takes the product's name and list price", () => {
    const fills = fillsFor(line, ref, product);
    expect(fills.map((f) => [f.field.name, f.source])).toEqual([
      ["unit_price_minor", "list_price_minor"],
      ["description", "name"],
    ]);
    expect(fillValues(fills, undefined, drill, () => "", asEdit)).toEqual({
      unit_price_minor: "12900",
      description: "Drill",
    });
  });
  it("keeps what the person typed, follows what the earlier product gave", () => {
    const fills = fillsFor(line, ref, product);
    const draft: Record<string, string> = { description: "Drill", unit_price_minor: "99" };
    expect(fillValues(fills, drill, saw, (n) => draft[n], asEdit)).toEqual({ description: "Saw" });
  });
  it("a purchase line takes the cost", () => {
    const po = model("purchase_order_line", [
      field("order_id", { kind: "ref", to: "purchase_order", required: true }),
      field("product_id", { kind: "ref", to: "product" }),
      field("unit_price", { kind: "money" }),
    ]);
    expect(fillsFor(po, po.fields[1]!, product).map((f) => f.source)).toEqual(["cost_minor"]);
  });
  it("the fill_from hint wins", () => {
    const hinted = model("x_line", [
      field("product_id", { kind: "ref", to: "product" }),
      field("label", { fill_from: "product_id.name" }),
    ]);
    expect(fillsFor(hinted, hinted.fields[0]!, product).map((f) => f.field.name)).toEqual([
      "label",
    ]);
  });
});

describe("which records a picker offers", () => {
  const milestone = model("projects_milestone", [
    field("name", { required: true }),
    field("project_id", { kind: "ref", to: "projects_project" }),
  ]);
  const task = model("projects_task", [
    field("name", { required: true }),
    field("project_id", { kind: "ref", to: "projects_project" }),
    field("milestone_id", { kind: "ref", to: "projects_milestone" }),
  ]);
  it("a task's milestone is one of the task's project", () => {
    const r = refConditions(task, task.fields[2]!, milestone, (n) =>
      n === "project_id" ? "pr1" : "",
    );
    expect(r.conds).toEqual([{ field: "project_id", op: "eq", value: "pr1" }]);
    expect(meets({ id: "m1", project_id: "pr1" }, r.conds)).toBe(true);
    expect(meets({ id: "m2", project_id: "pr2" }, r.conds)).toBe(false);
    expect(refConditions(task, task.fields[2]!, milestone, () => "").conds).toEqual([]);
  });
  it("reads a ref_filter", () => {
    expect(
      parseRefFilter("active = true && project_id = $project_id && kind != 'internal'", (n) =>
        n === "project_id" ? "pr1" : undefined,
      ),
    ).toEqual([
      { field: "active", op: "eq", value: true },
      { field: "project_id", op: "eq", value: "pr1" },
      { field: "kind", op: "ne", value: "internal" },
    ]);
  });
});
