/**
 * Where a long app name may break when a line is too short for it: at the seams of its compound, with
 * a soft hyphen (U+00AD), which is drawn only where the line breaks. At 125 percent text on a 375 px
 * phone the index broke three names inside a word ("Sähköpostimarkkinoin" / "ti", "E-" /
 * "postmarknadsföring", "Rechnungsstel-" / "lung") because the text column beside the icon was a few
 * pixels narrower than the whole word. A name is only changed here, for display: the title, the
 * Markdown copy and the search index keep the plain name.
 */
const SHY = "­";
const SEAMS: Record<string, string[]> = {
  Rechnungsstellung: ["Rechnungs", "stellung"],
  Sähköpostimarkkinointi: ["Sähkö", "posti", "markkinointi"],
  Varastonhallinta: ["Varaston", "hallinta"],
  Instrumentpaneler: ["Instrument", "paneler"],
  "E-postmarknadsföring": ["E-post", "marknads", "föring"],
};

export const softName = (name: string): string => SEAMS[name]?.join(SHY) ?? name;
/** The names that have a seam, for the test that checks the table against the real names. */
export const SOFT_NAMES = Object.keys(SEAMS);
