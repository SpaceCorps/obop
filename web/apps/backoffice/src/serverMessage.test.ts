import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage, type Lang } from "@obop/ui";
import { NETWORK_MESSAGE, serverFault } from "./net.ts";
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
