/**
 * The words and shapes that a person who does not build software cannot read, which the plain layer
 * of an app page (the large sentence, the paragraph under it, the cards that are visible without
 * opening a fold, the lines about AI agents, the strip) leaves to the part for developers. One
 * list, read by the unit tests and by the browser tests, so the two cannot disagree (a copy of it
 * once lost its `\b` to two backspace characters and could not fail).
 *
 * Every word of the list has an example that contains it, in the same row, so a word cannot be added
 * without a sentence that proves the rule finds it. The unit test pins the words themselves (a word
 * that is deleted from here makes it fail), and tries each example.
 */
export const TECHNICAL: readonly { word: string; example: string }[] = [
  { word: "API", example: "call the API with a token" },
  { word: "SKU", example: "no two products share a SKU" },
  { word: "KPI", example: "a single KPI on a dashboard" },
  { word: "hash(?:es)?", example: "a hash of the file is kept" },
  { word: "SHA-?\\d*", example: "takes the SHA-256 of the PDF" },
  { word: "payload", example: "the payload is never stored" },
  { word: "immutable", example: "an immutable record" },
  { word: "withheld", example: "the fields are withheld from agents" },
  { word: "allow-?list", example: "the field allowlist" },
  { word: "pseudonyms?", example: "every person gets a pseudonym" },
  { word: "approval-gated", example: "an approval-gated step" },
  { word: "anonymi[sz]e[sd]?", example: "Anonymise the party in place" },
  { word: "masked", example: "the email is masked" },
  { word: "clients?", example: "a client renders it" },
  { word: "schema", example: "the schema of the table" },
  { word: "quer(?:y|ies)", example: "the package never runs a query" },
  { word: "JSON", example: "returned as JSON" },
  { word: "endpoint", example: "call the endpoint" },
  // "data classes?" matched only "data classe" and "data classes": the unit test found it.
  { word: "data class(?:es)?", example: "each field has a data class" },
  { word: "widgets?", example: "a set of widgets" },
  { word: "aggregate", example: "how the numbers aggregate" },
  { word: "SLA", example: "A service-level SLA" },
  { word: "approvers?", example: "only the approver can do it" },
];

export const TECHNICAL_WORDS = new RegExp(`\\b(${TECHNICAL.map((t) => t.word).join("|")})\\b`, "i");

/** A field or group name (`signed_by`, `inventory.user`): code, not words. */
export const CODE_TOKENS = /\b[a-z]+_[a-z_]+\b|\b[a-z][a-z_]*\.(?:user|manager|approver|admin)\b/;

/**
 * The first six cards of a section, or all of them when there are seven: the fold the page had when it
 * showed six. It is no longer what a visitor sees without opening a fold. The page shows what
 * `splitCards` (scripts/lib/cards.ts) picks: every card with the approval mark or `keep`, then the
 * first of the others, three in all and at least one other, so a marked card from further down is
 * shown and a card among the first six can be folded. The test in cards.test.ts reads those cards
 * against the words above; this list is left as it was, so the older tests read at least as many
 * cards as they did.
 */
export const visible = <T>(cards: T[]): T[] => (cards.length > 7 ? cards.slice(0, 6) : cards);

/** Where a text breaks the rule: the first technical word or code token in it, or null. */
export function technicalIn(text: string): string | null {
  // The one place a code name is explained in the same sentence it appears in (Connectors).
  const plain = text.replace(/code name \(a pseudonym\)/, "code name");
  const hit = TECHNICAL_WORDS.exec(plain) ?? CODE_TOKENS.exec(plain);
  return hit ? hit[0] : null;
}
