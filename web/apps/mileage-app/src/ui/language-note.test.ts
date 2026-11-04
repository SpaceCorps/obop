import { describe, expect, test } from "vite-plus/test";
import { en } from "../i18n/en.ts";
import { sv } from "../i18n/sv.ts";
import { languageNoteKey } from "./language-note.ts";

describe("the note under the language choice", () => {
  test("only a choice that could not be saved has a note", () => {
    expect(languageNoteKey(false)).toBe("language.unsaved");
    expect(languageNoteKey(true)).toBeNull();
    expect(languageNoteKey(null)).toBeNull();
  });

  test("the sentence exists in both languages and does not name the language it returns to", () => {
    // After a failed save the next start reads an earlier saved choice if there is one, else the
    // phone's language: the note must not promise either (it once said "the language of the phone").
    for (const messages of [en, sv] as Record<string, string>[]) {
      const text = messages["language.unsaved"]!;
      expect(text.length).toBeGreaterThan(20);
      expect(text).not.toMatch(/\{/);
    }
    expect(en["language.unsaved"]).toContain(
      "may use the language that was saved before, or the language of the phone",
    );
    expect(sv["language.unsaved"]).toContain(
      "kan den använda det språk som sparades tidigare, eller telefonens språk",
    );
  });
});
