import type { MessageKey } from "../i18n/index.ts";

/**
 * The note the Account screen shows under the language choice. `stored` is what `choose` of the
 * language store answered: `true` when the choice was saved, `false` when it could not be (the
 * language is changed anyway, and the next start may read an earlier saved choice), `null` before
 * the person has chosen anything in this visit. Only a failed save has a note.
 */
export function languageNoteKey(stored: boolean | null): MessageKey | null {
  return stored === false ? "language.unsaved" : null;
}
