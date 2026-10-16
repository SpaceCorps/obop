import { Fragment, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { CodeBlock, EmptyState, Icon, Kbd, Notice, getLang, t } from "@obop/ui";
import source from "../../../../docs/MODULES.md?raw";
import { href } from "./route.ts";
import {
  headingNumber,
  parseMarkdown,
  searchSections,
  sections,
  toc,
  type Block,
  type Inline,
} from "./markdown.ts";

/** The guide is `docs/MODULES.md`, imported as text at build time and read once. */
const BLOCKS = parseMarkdown(source);
const TOC = toc(BLOCKS);
const SECTIONS = sections(BLOCKS);
const NUMBERS = new Map(
  BLOCKS.flatMap((b) => {
    if (b.t !== "heading") return [];
    const n = headingNumber(b.text);
    return n ? [[n, b.id] as const] : [];
  }),
);

const link = (id: string) => href({ app: "customize", model: "guide", tab: "guide", section: id });

type Props = { anchor: string; goAnchor: (id: string) => void; notify: (t: string) => void };

/** Words like "section 6.1" become links to that heading. */
function withSectionLinks(text: string, key: string, go: (id: string) => void): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(/\b([Ss]ections?\s+)(\d+(?:\.\d+)?)\b/g)) {
    const id = NUMBERS.get(m[2]!);
    if (!id) continue;
    const at = m.index! + m[1]!.length;
    out.push(text.slice(last, at));
    out.push(
      <a
        key={`${key}-${at}`}
        href={link(id)}
        onClick={(e) => {
          e.preventDefault();
          go(id);
        }}
      >
        {m[2]}
      </a>,
    );
    last = at + m[2]!.length;
  }
  out.push(text.slice(last));
  return out;
}

function Inlines({ c, go }: { c: Inline[]; go: (id: string) => void }): ReactNode {
  return c.map((x, i) => {
    switch (x.t) {
      case "text":
        return <Fragment key={i}>{withSectionLinks(x.v, String(i), go)}</Fragment>;
      case "code":
        return <code key={i}>{x.v}</code>;
      case "strong":
        return (
          <strong key={i}>
            <Inlines c={x.c} go={go} />
          </strong>
        );
      case "em":
        return (
          <em key={i}>
            <Inlines c={x.c} go={go} />
          </em>
        );
      case "link":
        return (
          <a
            key={i}
            href={x.href}
            {...(x.href.startsWith("#") ? {} : { target: "_blank", rel: "noreferrer noopener" })}
          >
            <Inlines c={x.c} go={go} />
          </a>
        );
    }
  });
}

function BlockView({
  b,
  go,
  copy,
}: {
  b: Block;
  go: (id: string) => void;
  copy: (id: string) => void;
}) {
  switch (b.t) {
    case "heading": {
      const Tag = `h${Math.min(b.level + 1, 6)}` as "h2";
      return (
        <Tag id={`g-${b.id}`} className={`guide-h${b.level}`} tabIndex={-1}>
          <Inlines c={b.c} go={go} />
          {b.level > 1 && (
            <a
              className="guide-anchor"
              href={link(b.id)}
              aria-label={t("Link to {section}", { section: b.text })}
              onClick={(e) => {
                e.preventDefault();
                copy(b.id);
              }}
            >
              <Icon name="link" size={14} />
            </a>
          )}
        </Tag>
      );
    }
    case "p":
      return (
        <p>
          <Inlines c={b.c} go={go} />
        </p>
      );
    case "list": {
      const Tag = b.ordered ? "ol" : "ul";
      return (
        <Tag>
          {b.items.map((it, i) => (
            <li key={i}>
              <Inlines c={it.c} go={go} />
              {it.sub && <BlockView b={it.sub} go={go} copy={copy} />}
            </li>
          ))}
        </Tag>
      );
    }
    case "table":
      return (
        <div className="table-wrap guide-table">
          <table className="table dense">
            <thead>
              <tr>
                {b.head.map((h, i) => (
                  <th key={i} scope="col" style={{ textAlign: b.align[i] ?? undefined }}>
                    <Inlines c={h} go={go} />
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {b.rows.map((r, i) => (
                <tr key={i}>
                  {r.map((c, j) => (
                    <td key={j} style={{ textAlign: b.align[j] ?? undefined }}>
                      <Inlines c={c} go={go} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case "code":
      return <CodeBlock code={b.text} lang={b.lang} />;
    case "quote":
      return (
        <blockquote>
          {b.c.map((x, i) => (
            <BlockView key={i} b={x} go={go} copy={copy} />
          ))}
        </blockquote>
      );
    case "hr":
      return <hr />;
  }
}

/** "How to write a module", inside the product: contents, search, deep links, copyable code. */
export function Guide({ anchor, goAnchor, notify }: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(anchor || TOC[0]?.id || "");
  // The contents sit beside the text on wide screens and fold away on phones.
  const wide = () => matchMedia("(min-width: 57rem)").matches;
  const [tocOpen, setTocOpen] = useState(wide);
  const root = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const hits = useMemo(() => searchSections(SECTIONS, query.trim()), [query]);
  const title = BLOCKS.find((b) => b.t === "heading" && b.level === 1);

  // Jump to the section named in the address, and again whenever it changes.
  useEffect(() => {
    if (!anchor) return;
    const el = document.getElementById(`g-${anchor}`);
    if (el) {
      el.scrollIntoView({ block: "start" });
      el.focus({ preventScroll: true });
      setActive(anchor);
    }
  }, [anchor, query]);

  // The contents mark the section being read.
  useEffect(() => {
    const scroller = root.current?.closest<HTMLElement>(".content");
    const heads = [...(root.current?.querySelectorAll<HTMLElement>("[id^='g-']") ?? [])].filter(
      (h) => h.matches("h3, h4"),
    );
    if (!scroller || !heads.length || query) return;
    const io = new IntersectionObserver(
      (entries) => {
        const seen = entries
          .filter((e) => e.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
        if (seen) setActive(seen.target.id.slice(2));
      },
      { root: scroller, rootMargin: "0px 0px -75% 0px" },
    );
    heads.forEach((h) => io.observe(h));
    return () => io.disconnect();
  }, [query]);

  // "/" focuses the search box, like most documentation sites.
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      if (e.key === "/" && !/^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName) && !el.isContentEditable) {
        e.preventDefault();
        search.current?.focus();
      }
    };
    addEventListener("keydown", on);
    return () => removeEventListener("keydown", on);
  }, []);

  const go = (id: string) => {
    setQuery("");
    if (!wide()) setTocOpen(false);
    goAnchor(id);
  };
  const copy = (id: string) => {
    goAnchor(id);
    const url = `${location.origin}${location.pathname}${link(id)}`;
    void navigator.clipboard?.writeText(url).then(
      () => notify(t("Link to this section copied")),
      () => {},
    );
  };

  return (
    <div className="guide" ref={root}>
      {getLang() !== "en" && !getLang().startsWith("en-") && (
        <Notice tone="info">
          {t(
            "The guide is written in English, like the module file format it describes. The rest of the screen is in your language.",
          )}
        </Notice>
      )}
      <div className="guide-top">
        <label className="searchfield guide-search">
          <Icon name="search" size={16} />
          <input
            ref={search}
            type="search"
            aria-label={t("Search the guide")}
            placeholder={t("Search the guide")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape" && query) {
                e.stopPropagation();
                setQuery("");
              }
            }}
          />
          <Kbd>/</Kbd>
        </label>
      </div>
      <div className="guide-body">
        <nav className="guide-toc" aria-label={t("Contents")}>
          <details open={tocOpen} onToggle={(e) => setTocOpen(e.currentTarget.open)}>
            <summary>{t("Contents")}</summary>
            <ol lang="en">
              {TOC.map((entry) => (
                <li key={entry.id} className={`lvl${entry.level}`}>
                  <a
                    href={link(entry.id)}
                    aria-current={active === entry.id ? "location" : undefined}
                    onClick={(e) => {
                      e.preventDefault();
                      go(entry.id);
                    }}
                  >
                    {entry.text}
                  </a>
                </li>
              ))}
            </ol>
          </details>
        </nav>
        <article
          className="guide-text"
          lang="en"
          aria-label={title && title.t === "heading" ? title.text : t("Guide")}
        >
          {query.trim() ? (
            <section aria-label={t("Search results")} lang={getLang()}>
              <h2 className="guide-h1">
                {t(
                  "{count, plural, one {# section matches “{query}”} other {# sections match “{query}”}}",
                  { count: hits.length, query: query.trim() },
                )}
              </h2>
              {hits.length === 0 ? (
                <EmptyState
                  icon="search"
                  title={t("Nothing matches")}
                  hint={t("Try a shorter word, such as approval, ref, state or filter.")}
                  action={{ label: t("Clear the search"), onClick: () => setQuery("") }}
                />
              ) : (
                <ul className="guide-hits">
                  {hits.map((h) => (
                    <li key={h.id}>
                      <a
                        href={link(h.id)}
                        onClick={(e) => {
                          e.preventDefault();
                          go(h.id);
                        }}
                      >
                        {h.title}
                      </a>
                      <p>{h.snippet}</p>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          ) : (
            BLOCKS.map((b, i) => <BlockView key={i} b={b} go={go} copy={copy} />)
          )}
        </article>
      </div>
    </div>
  );
}
