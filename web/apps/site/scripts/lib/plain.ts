/**
 * The words and shapes that a person who does not build software cannot read, which the plain layer
 * of an app page (the large sentence, the paragraph under it, the cards that are visible without
 * opening a fold, the lines about AI agents, the strip) leaves to the part for developers. One list,
 * read by the unit tests and by the browser tests, so the two cannot disagree (a copy of it once lost
 * its `\b` to two backspace characters and could not fail).
 */
export const TECHNICAL_WORDS =
  /\b(API|SKU|KPI|hash(?:es)?|SHA-?\d*|payload|immutable|withheld|allow-?list|pseudonyms?|approval-gated|anonymi[sz]e[sd]?|masked|clients?|schema|quer(?:y|ies)|JSON|endpoint|data classes?|widgets?|aggregate|SLA|approvers?)\b/i;

/** A field or group name (`signed_by`, `inventory.user`): code, not words. */
export const CODE_TOKENS = /\b[a-z]+_[a-z_]+\b|\b[a-z][a-z_]*\.(?:user|manager|approver|admin)\b/;

/**
 * The cards a visitor sees without opening a fold: the first six, or all of them when there are seven
 * (a fold is only made for two cards or more; AppCards.vue).
 */
export const visible = <T>(cards: T[]): T[] => (cards.length > 7 ? cards.slice(0, 6) : cards);

/** Where a text breaks the rule: the first technical word or code token in it, or null. */
export function technicalIn(text: string): string | null {
  // The one place a code name is explained in the same sentence it appears in (Connectors).
  const plain = text.replace(/code name \(a pseudonym\)/, "code name");
  const hit = TECHNICAL_WORDS.exec(plain) ?? CODE_TOKENS.exec(plain);
  return hit ? hit[0] : null;
}
