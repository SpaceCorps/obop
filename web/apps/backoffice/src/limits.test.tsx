import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage, type Lang } from "@obop/ui";
import type { FieldMeta } from "./api.ts";
import { fromEdit, validate } from "./edit.ts";
import { FieldInput } from "./fields.tsx";
import { codePoints, counterState, rangeError, textLimitError } from "./limits.ts";
import { registerMeta, serverMessage } from "./serverMessage.ts";

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

async function use(lang: Lang, messages: Record<string, string> = {}) {
  provideCatalogue(lang, messages);
  await setLanguage(lang, false);
}
afterEach(() => use("en"));

const EMOJI = "\u{1F600}";

describe("counting characters (D90)", () => {
  it("counts Unicode code points, as the server does", () => {
    expect(codePoints("")).toBe(0);
    expect(codePoints("abc")).toBe(3);
    expect(codePoints("åäö")).toBe(3);
    expect(codePoints(EMOJI)).toBe(1);
    expect(EMOJI.length).toBe(2);
    expect(codePoints(EMOJI.repeat(3))).toBe(3);
    // A family emoji is five code points (three people and two joiners), as in Rust's chars().
    expect(codePoints("\u{1F468}‍\u{1F469}‍\u{1F467}")).toBe(5);
    // A lone surrogate is one character, not two.
    expect(codePoints("\ud83d")).toBe(1);
  });
});

describe("length limits of a text field", () => {
  const note = field("note", "text", { max_length: 3, min_length: 2 });

  it("accept exactly the limit and refuse one more", () => {
    expect(textLimitError("Note", note, "abc")).toBeNull();
    expect(textLimitError("Note", note, "abcd")).toBe(
      "Note must be at most 3 characters; this has 4",
    );
  });

  it("count an emoji and a letter with a diaeresis once", () => {
    expect(textLimitError("Note", note, EMOJI.repeat(3))).toBeNull();
    expect(textLimitError("Note", note, "ååå")).toBeNull();
    expect(textLimitError("Note", note, EMOJI.repeat(4))).toBe(
      "Note must be at most 3 characters; this has 4",
    );
  });

  it("refuse a text shorter than the minimum, and say 1 character in the singular", () => {
    expect(textLimitError("Note", note, "a")).toBe(
      "Note must be at least 2 characters; this has 1",
    );
    const one = field("code", "text", { min_length: 1, max_length: 1 });
    expect(textLimitError("Code", one, "")).toBe("Code must be at least 1 character; this has 0");
    expect(textLimitError("Code", one, "ab")).toBe("Code must be at most 1 character; this has 2");
  });

  it("do not trim: the server counts what is sent", () => {
    expect(textLimitError("Note", note, "abc ")).toBe(
      "Note must be at most 3 characters; this has 4",
    );
    // Two characters, one of them a space: long enough, as the server counts it.
    expect(textLimitError("Note", note, "a ")).toBeNull();
  });

  it("have no limit to check when the field declares none and the server sent none", () => {
    expect(textLimitError("Note", field("note", "text"), "x".repeat(100_000))).toBeNull();
  });
});

describe("range limits of a number, an amount and a decimal", () => {
  it("hold a whole number to min and max, both inclusive", () => {
    const n = field("qty", "number", { min: 0, max: 100 });
    expect(rangeError("Qty", n, 0)).toBeNull();
    expect(rangeError("Qty", n, 100)).toBeNull();
    expect(rangeError("Qty", n, -1)).toBe("Qty must be at least 0; this is -1");
    expect(rangeError("Qty", n, 101)).toBe("Qty must be at most 100; this is 101");
  });

  it("compare an amount in minor units and show it in major units", () => {
    const m = field("price", "money", { min: 0, max: 100000 });
    expect(rangeError("Price", m, 100000, "EUR")).toBeNull();
    expect(rangeError("Price", m, 100001, "EUR")).toMatch(
      /^Price must be at most 1,?000\.00; this is 1,?000\.01$/,
    );
    expect(rangeError("Price", m, -5, "EUR")).toMatch(
      /^Price must be at least 0\.00; this is -0\.05$/,
    );
  });

  it("compare a decimal exactly, without a float", () => {
    const d = field("rate", "decimal", {
      precision: 12,
      scale: 7,
      min: "0.1000000",
      max: "0.9000000",
    });
    expect(rangeError("Rate", d, "0.1000000")).toBeNull();
    expect(rangeError("Rate", d, "0.1000001")).toBeNull();
    expect(rangeError("Rate", d, "0.0999999")).toMatch(
      /^Rate must be at least 0\.1000000; this is 0\.0999999$/,
    );
    expect(rangeError("Rate", d, "0.9000001")).toMatch(
      /^Rate must be at most 0\.9000000; this is 0\.9000001$/,
    );
    // These two are the same double; they are not the same decimal.
    expect(rangeError("Rate", d, "0.9000000")).toBeNull();
  });
});

describe("the edit rules with limits", () => {
  const note = field("note", "text", { max_length: 5, min_length: 2 });

  it("refuse a typed text outside the limits and keep what is inside", () => {
    expect(fromEdit(note, "abcdef")).toEqual({
      error: "Note must be at most 5 characters; this has 6",
    });
    expect(fromEdit(note, "abc")).toEqual({ value: "abc" });
    expect(fromEdit(note, "a")).toEqual({
      error: "Note must be at least 2 characters; this has 1",
    });
  });

  it("treat an emptied field as absent, whatever min_length says, as the form always did", () => {
    expect(fromEdit(note, "")).toEqual({ value: null });
    expect(fromEdit(note, "   ")).toEqual({ value: null });
    expect(fromEdit({ ...note, required: true }, "  ")).toEqual({ error: "Note is required" });
  });

  it("leave a value that was not changed alone, as the server does", () => {
    expect(fromEdit(note, "abcdefgh", undefined, true)).toEqual({ value: "abcdefgh" });
    expect(fromEdit(field("qty", "number", { max: 3 }), "9", undefined, true)).toEqual({
      value: 9,
    });
    // ... but the other checks still run.
    expect(fromEdit(field("qty", "number", { max: 3 }), "x", undefined, true)).toEqual({
      error: "Qty must be a whole number",
    });
  });

  it("check numbers, amounts and decimals when they are typed", () => {
    expect(fromEdit(field("qty", "number", { min: 1 }), "0")).toEqual({
      error: "Qty must be at least 1; this is 0",
    });
    expect(fromEdit(field("price", "money", { max: 1000 }), "10.01", "EUR")).toMatchObject({
      error: expect.stringMatching(/^Price must be at most 10\.00; this is 10\.01$/),
    });
    const rate = field("rate", "decimal", { precision: 6, scale: 2, max: "5.00" });
    expect(fromEdit(rate, "5.00")).toEqual({ value: "5.00" });
    expect(fromEdit(rate, "5.01")).toMatchObject({
      error: expect.stringContaining("at most 5.00"),
    });
  });

  it("validate a draft field by field and spare the unchanged ones", () => {
    const fields = [note, field("code", "text", { max_length: 2 })];
    const draft: Record<string, string> = { note: "abcdefg", code: "xyz" };
    const errs = validate(fields, (f) => draft[f.name]!);
    expect(Object.keys(errs)).toEqual(["note", "code"]);
    const spared = validate(
      fields,
      (f) => draft[f.name]!,
      undefined,
      undefined,
      (f) => f.name === "code",
    );
    expect(Object.keys(spared)).toEqual(["note"]);
  });
});

describe("the counter", () => {
  const note = field("note", "longtext", { max_length: 100 });

  it("stays away until the text is close to the limit", () => {
    expect(counterState(note, "")).toBeNull();
    expect(counterState(note, "x".repeat(79))).toBeNull();
    expect(counterState(note, "x".repeat(80))).toEqual({ used: 80, max: 100, over: false });
    expect(counterState(note, "x".repeat(100))).toEqual({ used: 100, max: 100, over: false });
    expect(counterState(note, "x".repeat(101))).toEqual({ used: 101, max: 100, over: true });
  });

  it("counts characters, not UTF-16 units", () => {
    // 80 emoji are 160 UTF-16 units but 80 characters: at the 80 % mark, not over the limit.
    expect(counterState(note, EMOJI.repeat(80))).toEqual({ used: 80, max: 100, over: false });
    expect(counterState(note, EMOJI.repeat(60))).toBeNull();
  });

  it("has nothing to count where the server sent no limit", () => {
    expect(counterState(field("note", "text"), "x".repeat(5000))).toBeNull();
  });
});

describe("the form control", () => {
  const html = (f: FieldMeta, value: string, extra: { readOnly?: boolean } = {}) =>
    renderToStaticMarkup(<FieldInput field={f} value={value} onChange={() => {}} {...extra} />);

  it("shows the counter of a long text near its limit, red above it", () => {
    const note = field("note", "longtext", { max_length: 10 });
    expect(html(note, "abc")).not.toContain("counter");
    expect(html(note, "12345678")).toMatch(/class="hint counter"[^>]*>8 of 10 characters</);
    const over = html(note, "12345678901");
    expect(over).toMatch(/class="hint counter over"[^>]*>11 of 10 characters</);
    // The counter is read with the field.
    const id = /<small class="hint counter over" id="([^"]+)"/.exec(over)![1];
    expect(over).toContain(`aria-describedby="${id}"`);
  });

  it("does not set the native maxlength, which would cut a pasted text without a word", () => {
    expect(html(field("note", "text", { max_length: 10 }), "abc")).not.toContain("maxlength");
    expect(html(field("note", "longtext", { max_length: 10 }), "abc")).not.toContain("maxlength");
  });

  it("gives a numeric input its bounds", () => {
    const out = html(field("qty", "number", { min: 0, max: 100 }), "5");
    expect(out).toContain('min="0"');
    expect(out).toContain('max="100"');
  });

  it("shows no counter in a field that cannot be edited", () => {
    const note = field("note", "text", { max_length: 10 });
    expect(html(note, "1234567890", { readOnly: true })).not.toContain("counter");
  });
});

describe("the server's own refusals", () => {
  const meta = [
    {
      id: "crm",
      name: "CRM",
      models: [
        {
          name: "party",
          description: "",
          can_create: true,
          can_update: true,
          fields: [
            field("name", "text", { max_length: 2000 }),
            field("limit", "money", { min: 0 }),
            field("rate", "decimal", { precision: 12, scale: 7, max: "0.9000000" }),
          ],
        },
      ],
    },
  ] as unknown as Parameters<typeof registerMeta>[0];

  it("are shown exactly as received in English", () => {
    registerMeta(meta);
    const m = "party.name must be at most 2000 characters; this has 2001";
    expect(serverMessage(m)).toBe(m);
  });

  it("are shown in the language of the person, from the real catalogues", async () => {
    registerMeta(meta);
    for (const [lang, expected] of Object.entries({
      de: [
        "Name darf höchstens 2.000 Zeichen lang sein; hier sind es 2.001",
        "Name muss mindestens 1 Zeichen lang sein; hier sind es 0",
        "Limit muss mindestens 0,00 sein; der Wert ist -5,00",
        "Rate darf höchstens 0,9000000 sein; der Wert ist 0,9000001",
      ],
      pl: [
        "Nazwa może mieć co najwyżej 2000 znaków; wpisano 2001",
        "Nazwa musi mieć co najmniej 1 znak; wpisano 0",
        "Limit musi wynosić co najmniej 0,00; wartość to -5,00",
        "Rate może wynosić co najwyżej 0,9000000; wartość to 0,9000001",
      ],
    })) {
      const cat = JSON.parse(
        readFileSync(new URL(`../locales/${lang}/messages.json`, import.meta.url), "utf8"),
      ) as Record<string, string>;
      await use(lang as Lang, cat);
      const got = [
        serverMessage("party.name must be at most 2000 characters; this has 2001"),
        serverMessage("party.name must be at least 1 character; this has 0"),
        serverMessage("party.limit must be at least 0; this is -500"),
        serverMessage("party.rate must be at most 0.9000000; this is 0.9000001"),
      ].map((s) => s.replace(/ | /g, " "));
      expect(got, lang).toEqual(expected.map((s) => s.replace(/ | /g, " ")));
    }
  });
});
