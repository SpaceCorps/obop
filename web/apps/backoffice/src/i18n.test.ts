import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage, t, type Lang } from "@obop/ui";
import type { FieldMeta } from "./api.ts";
import { decimalToMinor, optionLabel, plainText } from "./format.ts";
import { actionTitle, appTitle, fieldTitle, modelTitle } from "./labels.ts";
import { describeCond } from "./query.ts";
import { nounFor, summarize } from "./remove.ts";
import { registerMeta, serverMessage } from "./serverMessage.ts";

async function use(lang: Lang, messages: Record<string, string> = {}) {
  provideCatalogue(lang, messages);
  await setLanguage(lang, false);
}
afterEach(() => use("en"));

const field = (
  name: string,
  kind: FieldMeta["kind"],
  extra: Partial<FieldMeta> = {},
): FieldMeta => ({
  name,
  kind,
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});

describe("server messages", () => {
  it("are shown exactly as received in English", () => {
    expect(serverMessage("party.name is required")).toBe("party.name is required");
    expect(serverMessage("unknown field party.name")).toBe("unknown field party.name");
  });

  it("known patterns become translatable messages in other languages", async () => {
    await use("fr", { "{field} is required": "{field} est obligatoire" });
    expect(serverMessage("party.name is required")).toBe("Name est obligatoire");
    expect(serverMessage("lead.stage must be one of: new, won")).toBe(
      "Stage must be one of: New, Won",
    );
    expect(serverMessage("cannot delete party P: still referenced by sale_order (2): a, b")).toBe(
      "This Party cannot be deleted: other records still refer to it (Sale order (2): a, b)",
    );
    expect(serverMessage("something unforeseen")).toBe("something unforeseen");
    expect(serverMessage("")).toBe("");
  });

  it("names models and fields the way the interface does", async () => {
    registerMeta([
      {
        id: "crm",
        name: "CRM",
        models: [
          {
            name: "lead",
            description: "",
            labels: { fr: "Piste" },
            can_create: true,
            can_update: true,
            can_delete: true,
            actions: [],
            fields: [field("title", "text", { labels: { fr: "Intitulé" } })],
          },
        ],
      },
    ]);
    await use("fr");
    expect(serverMessage("lead.title is required")).toBe("Intitulé is required");
    expect(serverMessage("no lead 123")).toBe("No such Piste record");
    registerMeta([]);
  });
});

describe("labels", () => {
  it("fall back to the humanised English name", () => {
    expect(fieldTitle({ name: "expected_revenue_minor" })).toBe("Expected revenue");
    expect(appTitle({ name: "Fleet" })).toBe("Fleet");
    expect(modelTitle({ name: "fleet_vehicle" }, "fleet")).toBe("Vehicle");
    expect(actionTitle({ id: "lead.mark_won" })).toBe("Mark won");
  });

  it("use the client table, then the server's labels", async () => {
    await use("de", {
      "field\u0004Expected revenue": "Erwarteter Umsatz",
      "app\u0004CRM": "CRM",
      "option\u0004In service": "In Betrieb",
      "action\u0004Mark won": "Als gewonnen markieren",
    });
    expect(fieldTitle({ name: "expected_revenue_minor" })).toBe("Erwarteter Umsatz");
    expect(fieldTitle({ name: "expected_revenue_minor", labels: { de: "Umsatzerwartung" } })).toBe(
      "Umsatzerwartung",
    );
    expect(fieldTitle({ name: "unlisted_thing" })).toBe("Unlisted thing");
    expect(optionLabel("in_service")).toBe("In Betrieb");
    expect(actionTitle({ id: "lead.mark_won" })).toBe("Als gewonnen markieren");
  });
});

describe("amounts and sentences", () => {
  it("turns a canonical decimal into minor units by digits", () => {
    expect(decimalToMinor("12.50", 2)).toBe(1250);
    expect(decimalToMinor("12.5", 2)).toBe(1250);
    expect(decimalToMinor("-0.05", 2)).toBe(-5);
    expect(decimalToMinor("12.505", 2)).toBeNull();
    expect(decimalToMinor("12.500", 2)).toBe(1250);
    expect(decimalToMinor("1.5", 0)).toBeNull();
    expect(decimalToMinor("abc", 2)).toBeNull();
  });

  it("csv and search text keep a dot decimal whatever the locale", async () => {
    await use("de");
    const money = field("total_minor", "money");
    expect(plainText(money, 123456, () => "")).toBe("1234.56");
    expect(plainText(field("done", "bool"), true, () => "")).toBe("Yes");
  });

  it("describes a range with locale amounts and a stored canonical bound", async () => {
    await use("de");
    const money = field("total_minor", "money");
    const text = describeCond(
      { op: "range", f: "total_minor", min: "12.50", max: "1000" },
      [money],
      {
        refText: () => "",
        refIds: () => [],
      },
    );
    expect(text).toContain("12,50");
    expect(text).toContain("1.000,00");
  });

  it("counts records with a generic noun outside English", async () => {
    expect(nounFor("party", 2)).toBe("parties");
    await use("pl", {
      "{count, plural, one {record} other {records}}":
        "{count, plural, one {rekord} few {rekordy} many {rekordów} other {rekordu}}",
    });
    expect(nounFor("party", 1)).toBe("rekord");
    expect(nounFor("party", 3)).toBe("rekordy");
    expect(nounFor("party", 7)).toBe("rekordów");
    expect(t("{count} {noun} deleted", { count: 5, noun: nounFor("party", 5) })).toBe(
      "5 rekordów deleted",
    );
    expect(summarize([{ id: "a", label: "A", status: "failed" }], "party")).toBe("1 failed");
  });
});
