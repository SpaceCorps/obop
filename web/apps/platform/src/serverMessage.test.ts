import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage, type Lang } from "@obop/ui";
import { serverMessage } from "./serverMessage.ts";

// The control plane answers in English. A refused database call (D74) arrives as a code word
// (`{"error":"storage_busy"}`), which is a sentence in every language, English too: a person who
// signs up while the database is busy must not be shown `storage_busy`.
const LANGS: Lang[] = ["de", "fr", "pl", "fi", "sv", "es"];
const FAULTS = ["storage_busy", "storage_unavailable", "storage_timeout"];

const read = (lang: string): Record<string, string> =>
  JSON.parse(readFileSync(new URL(`../locales/${lang}/messages.json`, import.meta.url), "utf8"));

afterEach(async () => {
  await setLanguage("en", false);
});

describe("server messages of the control plane", () => {
  it("shows a refused database call as a sentence in English", async () => {
    await setLanguage("en", false);
    expect(serverMessage("storage_busy")).toBe("The server is busy. Try again in a moment.");
    expect(serverMessage("storage_unavailable")).toContain("cannot be reached");
    expect(serverMessage("storage_timeout")).toContain("took too long");
    // Its own messages are shown exactly as received.
    expect(serverMessage("invalid username or password")).toBe("invalid username or password");
  });

  for (const lang of LANGS) {
    it(`${lang}: the three code words are sentences, not the code and not English`, async () => {
      provideCatalogue(lang, read(lang));
      await setLanguage(lang, false);
      for (const raw of FAULTS) {
        const out = serverMessage(raw);
        expect(out, raw).not.toBe(raw);
        expect(/[{}]/.test(out), `${raw} -> ${out}`).toBe(false);
        expect(out, raw).not.toMatch(/Try again in a moment|took too long/);
      }
      expect(serverMessage("not signed in")).not.toBe("not signed in");
    });
  }
});
