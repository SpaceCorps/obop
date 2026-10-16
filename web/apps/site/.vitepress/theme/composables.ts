import { computed } from "vue";
import { useData } from "vitepress";
import { messages, type Locale, type Messages } from "../../i18n/index.ts";

export const GITHUB = "https://github.com/SpaceCorps/obop";

/** Locale, strings and link helper for the current page. Works during static rendering. */
export function useSite() {
  const { frontmatter, page } = useData();
  const fm = computed(() => frontmatter.value as Record<string, any>);
  const locale = computed<Locale>(() => (fm.value["locale"] as Locale | undefined) ?? "en");
  const m = computed<Messages>(() => messages[locale.value]);
  const prefix = computed(() => (locale.value === "en" ? "" : `/${locale.value}`));
  const href = (p: string) => (p.startsWith("/") && !/^\/(app|community|status)(\/|$)/.test(p) ? `${prefix.value}${p}` : p);
  return { fm, page, locale, m, prefix, href };
}
