import { de } from "./de.ts";
import { en } from "./en.ts";
import { es } from "./es.ts";
import { fi } from "./fi.ts";
import { fr } from "./fr.ts";
import { pl } from "./pl.ts";
import { sv } from "./sv.ts";
import { LOCALES, type Locale, type Messages } from "./types.ts";

export { DEFAULT_LOCALE, LOCALES } from "./types.ts";
export type { Locale, Messages } from "./types.ts";

export const messages: Record<Locale, Messages> = { en, de, fr, pl, fi, sv, es };

export const isLocale = (s: string): s is Locale => (LOCALES as readonly string[]).includes(s);
