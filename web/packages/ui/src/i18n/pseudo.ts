// Pseudo-localisation: every Latin letter becomes an accented look-alike and words grow by about 40%
// (vowels are doubled), so an unwrapped, clipped or hard-coded string shows up at a glance.
const FROM = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const TO = "áƀçðéƒĝĥíĵķļɱñöþǫŕšţúṽŵẋýžÁßÇÐÉƑĜĤÍĴĶĻṀÑÖÞǪŔŠŢÚṼŴẊÝŽ";
const MAP = new Map([...FROM].map((c, i) => [c, TO[i]!]));
const VOWELS = new Set("áéíöúý".split(""));

/** Rewrite visible text; rich-text tags (<b>), and ICU-free text only. */
export function pseudo(text: string): string {
  return text
    .split(/(<\/?[a-z][a-z0-9]*>)/)
    .map((chunk, i) => (i % 2 ? chunk : chunk.replace(/[A-Za-z]+/g, word)))
    .join("");
}

function word(w: string): string {
  const out = [...w].map((c) => MAP.get(c) ?? c);
  if (w.length < 3) return out.join("");
  // Double the vowels once: "Contacts" -> "Çöññtáctš" becomes "Çööñtááctš".
  return out.map((c) => (VOWELS.has(c.toLowerCase()) ? c + c : c)).join("");
}

/** Is this text (a visible text node) already pseudo-localised? Used by the e2e check. */
export const hasPseudoChars = (s: string) => /[áƀçðéƒĝĥíĵķļɱñöþǫŕšţúṽŵẋýžßƑṀ]/i.test(s);
