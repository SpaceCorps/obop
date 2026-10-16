import { Fragment, useSyncExternalStore, type ReactNode } from "react";
import { LANGS, LANG_INFO, REGIONS, isPseudo, type Lang } from "./locales.ts";
import { formatNodes, parseMessage, type FormatEnv } from "./message.ts";
import {
  formatLocale,
  getCurrency,
  getDir,
  getLang,
  getRegion,
  getVersion,
  pluralOf,
  rawMessage,
  resetLanguage,
  setLanguage,
  setRegion,
  stringEnv,
  subscribe,
  personalLang,
} from "./store.ts";
import { t } from "./store.ts";
import { Icon } from "../icons.tsx";
import { Menu, Select } from "../primitives.tsx";

/** Re-renders the whole tree when the language, region or currency changes (remounts under a new key). */
export function I18nProvider({ children }: { children: ReactNode }) {
  useSyncExternalStore(subscribe, getVersion);
  return <Fragment key={`${getLang()}|${getRegion()}|${getCurrency()}`}>{children}</Fragment>;
}

/** Language state for components that show or change it. Text itself uses the global `t()`. */
export function useI18n() {
  useSyncExternalStore(subscribe, getVersion);
  return {
    lang: getLang(),
    region: getRegion(),
    dir: getDir(),
    locale: formatLocale(),
    personal: personalLang(),
    setLanguage,
    setRegion,
    resetLanguage,
  };
}

/* ================= Rich text ================= */

export type RichTags = Record<string, (children: ReactNode) => ReactNode>;
type RichVars = Record<string, unknown>;

const richEnv: FormatEnv<ReactNode> = {
  plural: pluralOf,
  number: stringEnv.number,
  value: (v) => (v instanceof Date || Array.isArray(v) ? stringEnv.value(v) : (v as ReactNode)),
  text: stringEnv.text,
  join: (parts) => parts as unknown as ReactNode,
};

function flatten(x: unknown, out: (string | ReactNode)[] = []): (string | ReactNode)[] {
  if (Array.isArray(x)) for (const i of x) flatten(i, out);
  else out.push(x as ReactNode);
  return out;
}

/**
 * A message with tags, for example "Press &lt;kbd&gt;Esc&lt;/kbd&gt; to cancel", with one renderer per tag.
 * Tags and `{placeholders}` may be reordered by a translation. Never renders HTML from the string.
 */
export function rich(
  id: string,
  vars: RichVars = {},
  tags: RichTags = {},
  ctx?: string,
): ReactNode {
  const text = rawMessage(id, ctx);
  const items = flatten(formatNodes(parseMessage(text), vars, richEnv));
  type Open = { name: string; kids: ReactNode[] };
  const root: Open = { name: "", kids: [] };
  const stack: Open[] = [root];
  const top = () => stack[stack.length - 1]!;
  let key = 0;
  const push = (n: ReactNode) => top().kids.push(<Fragment key={key++}>{n}</Fragment>);
  for (const it of items) {
    if (typeof it !== "string") {
      push(it);
      continue;
    }
    for (const piece of it.split(/(<\/?[a-z][a-z0-9]*>)/)) {
      const m = /^<(\/?)([a-z][a-z0-9]*)>$/.exec(piece);
      if (!m) {
        if (piece) push(piece);
      } else if (!m[1]) stack.push({ name: m[2]!, kids: [] });
      else if (stack.length > 1 && top().name === m[2]) {
        const done = stack.pop()!;
        const wrap = tags[done.name];
        push(wrap ? wrap(done.kids) : done.kids);
      }
    }
  }
  while (stack.length > 1) {
    const done = stack.pop()!;
    push(done.kids);
  }
  return <>{root.kids}</>;
}

/* ================= Language and region pickers ================= */

/** Languages a person can pick: the seven that ship, plus a pseudo-locale while one is active. */
export function languageOptions(current: Lang): Lang[] {
  return isPseudo(current) ? [...LANGS, current] : [...LANGS];
}

/** A compact language switcher for a header: a menu of the shipped languages, each in itself. */
export function LanguageMenu({ align = "end" }: { align?: "start" | "end" }) {
  const i18n = useI18n();
  return (
    <Menu
      ariaLabel={t("Language")}
      align={align}
      triggerClassName="btn ghost"
      trigger={
        <>
          <Icon name="website" size={16} />
          <span>{LANG_INFO[i18n.lang].native}</span>
        </>
      }
      items={languageOptions(i18n.lang).map((l) => ({
        label: LANG_INFO[l].native,
        icon: l === i18n.lang ? ("check" as const) : undefined,
        onSelect: () => void i18n.setLanguage(l),
      }))}
    />
  );
}

/** A labelled select for the interface language; each language is named in itself. */
export function LanguageSelect({
  value,
  onChange,
  id,
  label,
  hint,
}: {
  value: Lang;
  onChange: (l: Lang) => void;
  id?: string;
  label?: ReactNode;
  hint?: ReactNode;
}) {
  return (
    <Select
      id={id}
      label={label ?? t("Language")}
      hint={hint}
      value={value}
      onChange={(e) => onChange(e.target.value as Lang)}
    >
      {languageOptions(value).map((l) => (
        <option key={l} value={l} lang={isPseudo(l) ? "en" : l}>
          {LANG_INFO[l].native}
        </option>
      ))}
    </Select>
  );
}

/** Region (format) select: "Same as language", then each region with an example number. */
export function RegionSelect({
  value,
  onChange,
  id,
  label,
  hint,
}: {
  value: string;
  onChange: (r: string) => void;
  id?: string;
  label?: ReactNode;
  hint?: ReactNode;
}) {
  const lang = getLang();
  const names = (() => {
    try {
      return new Intl.DisplayNames([isPseudo(lang) ? "en" : lang], { type: "region" });
    } catch {
      return null;
    }
  })();
  const sample = (loc: string) =>
    new Intl.NumberFormat(loc, { minimumFractionDigits: 2 }).format(1234.5);
  return (
    <Select
      id={id}
      label={label ?? t("Region and formats")}
      hint={hint}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      <option value="auto">{t("Same as language")}</option>
      {REGIONS.map((r) => {
        const country = r.split("-")[1]!;
        return (
          <option key={r} value={r}>
            {names?.of(country) ?? country} ({sample(r)})
          </option>
        );
      })}
    </Select>
  );
}
