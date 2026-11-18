/**
 * Where a long app name may break in the top bar of a phone: at the seams of its compound, with a
 * soft hyphen (U+00AD), drawn only where the line breaks. At 320 px the bar has room for about 11
 * letters, and the browser cannot hyphenate on its own where it has no dictionary, so German
 * "Rechnungsstellung" read "Rechnungsst..." (docs/ISSUES.md Q18). The same seams as the website's
 * app index (`web/apps/site/.vitepress/theme/softname.ts`). Names of several words wrap at a space.
 */
const SHY = String.fromCharCode(0xad);
const SEAMS: Record<string, string[]> = {
  Rechnungsstellung: ["Rechnungs", "stellung"],
  Sähköpostimarkkinointi: ["Sähkö", "posti", "markkinointi"],
  Varastonhallinta: ["Varaston", "hallinta"],
  Instrumentpaneler: ["Instrument", "paneler"],
  "E-postmarknadsföring": ["E-post", "marknads", "föring"],
  Kestotilaukset: ["Kesto", "tilaukset"],
  Ajopäiväkirja: ["Ajo", "päivä", "kirja"],
  Tuntiseuranta: ["Tunti", "seuranta"],
  Verkkosivusto: ["Verkko", "sivusto"],
  Ajanvaraukset: ["Ajan", "varaukset"],
  Zeiterfassung: ["Zeit", "erfassung"],
  Fordonsflotta: ["Fordons", "flotta"],
};

/** The name with soft hyphens at its seams, for a line too short for it; other names as they are. */
export function seamedName(name: string): string {
  return name
    .split(" ")
    .map((w) => SEAMS[w]?.join(SHY) ?? w)
    .join(" ");
}
