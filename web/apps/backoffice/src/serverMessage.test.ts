import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage, type Lang } from "@obop/ui";
import { NETWORK_MESSAGE, serverFault } from "./net.ts";
import type { FieldMeta, ModelMeta, ModuleMeta } from "./api.ts";
import { registerMeta, serverMessage, storageFaultMessage } from "./serverMessage.ts";

// Every stable server message pattern (docs/MODULES.md sections 3.5, 4.1, 6.1 and the D55 install
// errors) is shown in the person's language. For each language the real catalogue is loaded and every
// sample must come out translated: not as received, with no placeholder left over.
const LANGS: Lang[] = ["de", "fr", "pl", "fi", "sv", "es"];

const SAMPLES: string[] = [
  NETWORK_MESSAGE,
  serverFault(500),
  "storage_busy",
  "storage_unavailable",
  "storage_timeout",
  'lead.stage starts as "new" when a record is created; it moves on through the model\'s actions',
  'lead.stage: moving from "new" to "won" is the mark_won action, which needs the crm.manager group',
  'stage must be new to do this, it is "won"',
  'unknown op "like" (use eq, ne, contains, in, gt, gte, lt, lte, is_null)',
  'unknown option "sort" (use filter, order, limit, offset, aggregate and lang)',
  "at most 50 filter conditions",
  "at most 5 order keys",
  "limit must be an integer from 1 to 5000",
  "offset must be zero or more",
  "filter on lead.stage: contains only works on text fields",
  "filter on lead.title: value must be a string",
  "aggregate would produce more than 10000 groups; narrow the filter or group by a coarser field",
  "aggregate is only available on search",
  "helpdesk needs hr >=0.2, which is not installed and not offered by the marketplace",
  "helpdesk needs hr >=0.2, <1, but the marketplace offers hr 0.1.0",
  "fleet 0.1.0 needs core ^0.9, you run 0.2.0",
  "dependency cycle: a -> b -> a",
  "fleet 0.1.0 is already installed",
  "fleet 0.1.0 is already installed; 0.0.9 would be a downgrade, which is refused",
  "crm is a built-in module and is part of the core",
  "model fleet_vehicle already exists",
  "base is required by crm (and, through them, sales); uninstall those first",
  "upgrading hr to 0.3.0 would break helpdesk, which needs hr ^0.2",
  "invalid obop.toml: expected an equals sign at line 3",
  "manifest is larger than 256 KiB",
  'module id "Fleet" must be lowercase letters, digits and _, starting with a letter',
  "version must look like 1.2.3 (got x)",
  "summary must be one line of at most 140 characters",
  'homepage must be an https:// URL (got "http://x")',
  "module needs a name and a description",
  "declarative packages run no code and may not declare network destinations",
  "package is too large (more than 50 models or 200 fields)",
  'model "x" must be "fleet" or start with "fleet_"',
  'permission group "x" must look like module.role',
  "fleet_vehicle.state: a select needs options",
  "fleet_vehicle.owner: a ref needs `to`, the target model",
  "fleet_vehicle.plate: field has no description",
];

const read = (lang: string): Record<string, string> =>
  JSON.parse(readFileSync(new URL(`../locales/${lang}/messages.json`, import.meta.url), "utf8"));

afterEach(async () => {
  registerMeta([]);
  await setLanguage("en", false);
});

describe("server messages in every language", () => {
  it("are shown exactly as received in English", async () => {
    await setLanguage("en", false);
    // (A refused database call is a code word, which is a sentence in English too: see below.)
    for (const raw of SAMPLES.filter((s) => !s.startsWith("storage_")))
      expect(serverMessage(raw)).toBe(raw);
  });

  for (const lang of LANGS) {
    it(`${lang}: every known pattern is translated`, async () => {
      provideCatalogue(lang, read(lang));
      await setLanguage(lang, false);
      const untranslated: string[] = [];
      for (const raw of SAMPLES) {
        const out = serverMessage(raw);
        if (out === raw || /[{}]/.test(out)) untranslated.push(`${raw} -> ${out}`);
      }
      expect(untranslated).toEqual([]);
    });
  }

  it("shows a refused database call as a sentence, in English too, and not as a wrong password", async () => {
    await setLanguage("en", false);
    const busy = "The server is busy. Try again in a moment.";
    expect(serverMessage("storage_busy")).toBe(busy);
    expect(serverMessage("storage_timeout")).toContain("took too long");
    // The sign-in form decides by status: a 503 is the server's trouble, whatever else the form
    // would have said about a failed sign-in.
    expect(storageFaultMessage(503, "storage_busy")).toBe(busy);
    expect(storageFaultMessage(503, "storage_unavailable")).toContain("cannot be reached");
    expect(storageFaultMessage(401, "")).toBeNull();
    expect(storageFaultMessage(503, "something_else")).toBeNull();
    expect(storageFaultMessage(503, undefined)).toBeNull();
  });

  it("names modules, fields and options the way the interface does", async () => {
    provideCatalogue("de", read("de"));
    await setLanguage("de", false);
    registerMeta([
      {
        id: "crm",
        name: "CRM",
        labels: { de: "Kundenpflege" },
        models: [],
      },
    ]);
    const out = serverMessage("crm is a built-in module and is part of the core");
    expect(out).toContain("Kundenpflege");
    expect(
      serverMessage(
        'lead.stage: moving from "new" to "won" is the mark_won action, which needs the crm.manager group',
      ),
    ).toContain("crm.manager");
  });
});

// A package that carries its own option words (`option_labels` in /api/meta, D52) must see them in
// every refused-change message, not the client's generic option table. The generic table of the
// real Swedish catalogue says "Ny" for new and "Stängd" for closed, and has no entry for
// "Under review"; the package below says something else for each.
describe("server messages use the package's own option labels", () => {
  const status = (extra: Partial<FieldMeta> = {}): FieldMeta => ({
    name: "status",
    kind: "select",
    description: "",
    class: "none",
    personal_data: false,
    required: false,
    options: ["new", "under_review", "closed"],
    labels: { sv: "Läge" },
    option_labels: {
      new: { sv: "Öppnad" },
      under_review: { sv: "Granskas" },
      closed: { sv: "Avslutad" },
    },
    ...extra,
  });
  const owner: FieldMeta = {
    name: "owner",
    kind: "text",
    description: "",
    class: "none",
    personal_data: false,
    required: true,
    labels: { sv: "Ansvarig" },
  };
  const model = (name: string, fields: FieldMeta[]): ModelMeta => ({
    name,
    description: "",
    can_create: true,
    can_update: true,
    can_delete: true,
    actions: [],
    fields,
  });
  const desk = (...models: ModelMeta[]): ModuleMeta[] => [{ id: "desk", name: "Desk", models }];
  /** The words of the generic option table that the package's words replace. */
  const GENERIC = [/\bNy\b/, /\bStängd\b/, /Under review/, /\bUnder_review\b/];

  async function swedish() {
    provideCatalogue("sv", read("sv"));
    await setLanguage("sv", false);
  }
  function packageWordsOnly(out: string, words: string[]) {
    for (const w of words) expect(out).toContain(w);
    for (const g of GENERIC) expect(out).not.toMatch(g);
  }

  it("in a refused move, with the model and field named by the sentence", async () => {
    await swedish();
    registerMeta(desk(model("desk_ticket", [status(), owner])));
    const refused = serverMessage(
      'desk_ticket.status cannot change from "new" to "closed"; from "new" it can only go to: under_review',
    );
    expect(refused).toBe(
      "Läge kan inte ändras från Öppnad till Avslutad; från Öppnad kan det bara ändras till: Granskas",
    );
    const dead = serverMessage(
      'desk_ticket.status cannot change from "closed" to "new": no transition leaves "closed"',
    );
    expect(dead).toBe(
      "Läge kan inte ändras från Avslutad till Öppnad: ingen åtgärd leder vidare från Avslutad",
    );
    packageWordsOnly(refused + dead, ["Öppnad", "Avslutad", "Granskas"]);
  });

  it("for a start value, an action's group and the list of allowed values", async () => {
    await swedish();
    registerMeta(desk(model("desk_ticket", [status(), owner])));
    const starts = serverMessage(
      'desk_ticket.status starts as "new" when a record is created; it moves on through the model\'s actions',
    );
    expect(starts).toContain("Läge börjar som Öppnad");
    const moving = serverMessage(
      'desk_ticket.status: moving from "under_review" to "closed" is the close action, which needs the desk.agent group',
    );
    expect(moving).toBe(
      "Läge: att gå från Granskas till Avslutad är åtgärden Stäng, som kräver gruppen desk.agent",
    );
    const one = serverMessage("desk_ticket.status must be one of: new, under_review, closed");
    expect(one).toBe("Läge måste vara något av: Öppnad, Granskas, Avslutad");
    const empty = serverMessage(
      'desk_ticket.status: moving from (empty) to "closed" is the close action, which needs the desk.agent group',
    );
    expect(empty).toContain("från tom till Avslutad");
    packageWordsOnly(starts + moving + one + empty, ["Öppnad", "Granskas", "Avslutad"]);
  });

  it("for the state a missing required field blocks", async () => {
    await swedish();
    registerMeta(desk(model("desk_ticket", [status(), owner])));
    const out = serverMessage(
      'desk_ticket.owner is required and must be set before status can change to "closed"',
    );
    expect(out).toBe("Ansvarig är obligatoriskt innan Läge kan ändras till Avslutad");
    packageWordsOnly(out, ["Avslutad"]);
  });

  it("when the sentence names only the field, found in the modules' metadata", async () => {
    await swedish();
    registerMeta(desk(model("desk_ticket", [status(), owner])));
    const out = serverMessage('status must be new to do this, it is "under_review"');
    expect(out).toBe("Läge måste vara Öppnad för att göra detta, det är Granskas");
    packageWordsOnly(out, ["Öppnad", "Granskas"]);
    // The server writes an unset value as the bare word, not as a quoted option.
    expect(serverMessage("status must be new to do this, it is empty")).toBe(
      "Läge måste vara Öppnad för att göra detta, det är tom",
    );
  });

  it("when several models share the field name and agree on the words", async () => {
    await swedish();
    registerMeta(
      desk(model("desk_ticket", [status(), owner]), model("desk_task", [status(), owner])),
    );
    expect(serverMessage('status must be new to do this, it is "closed"')).toBe(
      "Läge måste vara Öppnad för att göra detta, det är Avslutad",
    );
  });

  it("keeps the shared option words but not a title the models do not share", async () => {
    await swedish();
    // The same option words, but one model calls the field "Läge" and the other "Fas": the
    // sentence does not say which model it means, so it must not name the first one's title.
    registerMeta(
      desk(
        model("desk_ticket", [status(), owner]),
        model("desk_task", [status({ labels: { sv: "Fas" } }), owner]),
      ),
    );
    const out = serverMessage('status must be new to do this, it is "closed"');
    expect(out).toBe("Status måste vara Öppnad för att göra detta, det är Avslutad");
    expect(out).not.toMatch(/Läge|Fas/);
    // Either order of the models gives the same answer.
    registerMeta(
      desk(
        model("desk_task", [status({ labels: { sv: "Fas" } }), owner]),
        model("desk_ticket", [status(), owner]),
      ),
    );
    expect(serverMessage('status must be new to do this, it is "closed"')).toBe(out);
    // A title is the same in every model that has the field: it is used.
    registerMeta(
      desk(
        model("desk_ticket", [status(), owner]),
        model("desk_task", [status({ labels: { sv: "Läge" } }), owner]),
      ),
    );
    expect(serverMessage('status must be new to do this, it is "closed"')).toBe(
      "Läge måste vara Öppnad för att göra detta, det är Avslutad",
    );
    // Titles that differ only in another language do not matter in this one.
    registerMeta(
      desk(
        model("desk_ticket", [status(), owner]),
        model("desk_task", [status({ labels: { sv: "Läge", fi: "Tila" } }), owner]),
      ),
    );
    expect(serverMessage('status must be new to do this, it is "closed"')).toContain("Läge måste");
  });

  it("falls back to the generic table when the field cannot be known", async () => {
    await swedish();
    // Unknown model, and unknown field of a known model.
    registerMeta(desk(model("desk_ticket", [status(), owner])));
    expect(serverMessage("other.status must be one of: new, closed")).toBe(
      "Status måste vara något av: Ny, Stängd",
    );
    expect(serverMessage("desk_ticket.phase must be one of: new, closed")).toBe(
      "Phase måste vara något av: Ny, Stängd",
    );
    // No metadata at all.
    registerMeta([]);
    expect(
      serverMessage(
        'desk_ticket.status cannot change from "new" to "closed"; from "new" it can only go to: under_review',
      ),
    ).toContain("från Ny till Stängd; från Ny kan det bara ändras till: Under review");
    // Models that give the same value different words: the field is unknown, not guessed.
    registerMeta(
      desk(
        model("desk_ticket", [status(), owner]),
        model("desk_task", [status({ option_labels: { new: { sv: "Planerad" } } }), owner]),
      ),
    );
    expect(serverMessage('status must be new to do this, it is "closed"')).toContain(
      "måste vara Ny för att göra detta, det är Stängd",
    );
  });

  it("falls back per option: only the options without a package word use the generic table", async () => {
    await swedish();
    registerMeta(
      desk(model("desk_ticket", [status({ option_labels: { new: { sv: "Öppnad" } } }), owner])),
    );
    expect(serverMessage("desk_ticket.status must be one of: new, closed")).toBe(
      "Läge måste vara något av: Öppnad, Stängd",
    );
  });

  it("ignores another language's package words", async () => {
    provideCatalogue("de", read("de"));
    await setLanguage("de", false);
    registerMeta(desk(model("desk_ticket", [status(), owner])));
    const out = serverMessage("desk_ticket.status must be one of: new, closed");
    expect(out).not.toContain("Öppnad");
    expect(out).not.toContain("Avslutad");
  });
});

// B12: a refusal reads like a person wrote it, in English too: no engine suffix, no technical names.
describe("server messages without engine text", () => {
  const field = (name: string, labels?: Record<string, string>): FieldMeta => ({
    name,
    kind: "ref",
    description: "",
    class: "none",
    personal_data: false,
    required: true,
    ...(labels ? { labels } : {}),
  });
  const order: ModelMeta = {
    name: "sale_order",
    description: "",
    can_create: true,
    can_update: true,
    can_delete: true,
    fields: [field("customer_id", { en: "Customer" })],
    actions: [],
  };

  it("drops the assertion suffix", async () => {
    await setLanguage("en", false);
    expect(
      serverMessage(
        "An invoice with no amount cannot be posted (assertion total_minor > 0 failed)",
      ),
    ).toBe("An invoice with no amount cannot be posted");
    expect(
      serverMessage("Payments do not cover the total (assertion sum(x) >= total failed)"),
    ).toBe("Payments do not cover the total");
    expect(
      serverMessage(
        "The resource is already booked at that time: pick another time or another resource (rule no_double_booking failed)",
      ),
    ).toBe("The resource is already booked at that time: pick another time or another resource");
  });

  it("names the field by its label in English", async () => {
    await setLanguage("en", false);
    registerMeta([{ id: "sales", name: "Sales", models: [order] }]);
    expect(serverMessage("sale_order.customer_id is required")).toBe("Customer is required");
    expect(serverMessage("sale_order.note is required")).toBe("Note is required");
  });

  it("says what format or which days a field takes (K45)", async () => {
    await setLanguage("en", false);
    registerMeta([{ id: "sales", name: "Sales", models: [order] }]);
    expect(
      serverMessage(
        "sale_order.currency must be a currency code of three capital letters (ISO 4217), such as EUR",
      ),
    ).toBe("Currency must be a currency code of three capital letters, such as EUR");
    expect(
      serverMessage("sale_order.iban must be a valid IBAN, such as DE89 3704 0044 0532 0130 00"),
    ).toBe("IBAN must be a valid IBAN, such as DE89 3704 0044 0532 0130 00");
    expect(
      serverMessage(
        "sale_order.vat must be a VAT number with its country prefix, such as DE123456789",
      ),
    ).toBe("VAT must be a VAT number with its country prefix, such as DE123456789");
    const below = serverMessage(
      "sale_order.ordered_on must be at least 1900-01-01; this is 0313-12-13",
    );
    expect(below).toMatch(/^Ordered on must be at least .+1900; this is .+313$/);
  });

  it("says what was wrong inside a failed step of an action", async () => {
    await setLanguage("en", false);
    registerMeta([{ id: "sales", name: "Sales", models: [order] }]);
    expect(
      serverMessage(
        "sale_order.confirm: effect 1 (create) failed, nothing was changed: sale_order.customer_id is required",
      ),
    ).toBe("Customer is required");
  });
});

describe("paying an invoice that is not in the ledger", () => {
  const raw =
    "the invoice is not in the ledger yet (it has no move), so no receivable was booked for it and 1400 cannot be credited with its payment; book the invoice first (invoice_posting.create_move) and match its receivable item, or book the payment to a revenue account (nothing was written)";

  it("tells a person to choose another account, without the command name", async () => {
    await setLanguage("en", false);
    const out = serverMessage(raw);
    expect(out).toContain("1400 is a receivable");
    expect(out).not.toContain("invoice_posting");
  });

  for (const lang of LANGS)
    it(`${lang}: is translated`, async () => {
      provideCatalogue(lang, read(lang));
      await setLanguage(lang, false);
      const out = serverMessage(raw);
      expect(out).toContain("1400");
      expect(out).not.toContain("invoice_posting");
      expect(out).not.toMatch(/[{}]/);
    });
});
