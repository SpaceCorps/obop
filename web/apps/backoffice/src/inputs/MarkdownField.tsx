// Markdown text (plan 2.7 item 3): rendered when it is only read; while it is edited, a toolbar
// (heading, bold, italic, list, numbered list, link, quote), a live preview beside the text where
// there is room (or a Write and Preview switch where there is not), and "Edit full screen". The
// renderer is markdown.ts: it makes a tree and never HTML, so nothing typed can inject markup.
import { Fragment, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { Dialog, t } from "@obop/ui";
import { fieldTitle } from "../labels.ts";
import { parseMarkdown, type Block, type Inline } from "../markdown.ts";
import { Icon } from "../ui.tsx";
import type { BaseInputProps } from "./common.ts";

function Inlines({ c }: { c: Inline[] }): ReactNode {
  return c.map((x, i) => {
    switch (x.t) {
      case "text":
        return <Fragment key={i}>{x.v}</Fragment>;
      case "code":
        return <code key={i}>{x.v}</code>;
      case "strong":
        return (
          <strong key={i}>
            <Inlines c={x.c} />
          </strong>
        );
      case "em":
        return (
          <em key={i}>
            <Inlines c={x.c} />
          </em>
        );
      case "link":
        return (
          <a
            key={i}
            href={x.href}
            {...(x.href.startsWith("#") ? {} : { target: "_blank", rel: "noreferrer noopener" })}
          >
            <Inlines c={x.c} />
          </a>
        );
    }
  });
}

function BlockView({ b }: { b: Block }): ReactNode {
  switch (b.t) {
    case "heading": {
      const Tag = `h${Math.min(b.level + 2, 6)}` as "h3";
      return (
        <Tag className={`md-h${b.level}`}>
          <Inlines c={b.c} />
        </Tag>
      );
    }
    case "p":
      return (
        <p>
          <Inlines c={b.c} />
        </p>
      );
    case "list": {
      const Tag = b.ordered ? "ol" : "ul";
      return (
        <Tag>
          {b.items.map((it, i) => (
            <li key={i}>
              <Inlines c={it.c} />
              {it.sub && <BlockView b={it.sub} />}
            </li>
          ))}
        </Tag>
      );
    }
    case "table":
      return (
        <div className="md-table">
          <table>
            <thead>
              <tr>
                {b.head.map((h, i) => (
                  <th key={i} style={{ textAlign: b.align[i] ?? undefined }}>
                    <Inlines c={h} />
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {b.rows.map((r, i) => (
                <tr key={i}>
                  {r.map((c, j) => (
                    <td key={j} style={{ textAlign: b.align[j] ?? undefined }}>
                      <Inlines c={c} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case "code":
      return (
        <pre>
          <code>{b.text}</code>
        </pre>
      );
    case "quote":
      return (
        <blockquote>
          {b.c.map((x, i) => (
            <BlockView key={i} b={x} />
          ))}
        </blockquote>
      );
    case "hr":
      return <hr />;
  }
}

/** Markdown drawn as text with headings, lists, links and tables. */
export function MarkdownView({ text, className = "" }: { text: string; className?: string }) {
  const blocks = useMemo(() => parseMarkdown(text), [text]);
  if (!text.trim()) return <p className={`md dim ${className}`}>—</p>;
  return (
    <div className={`md ${className}`}>
      {blocks.map((b, i) => (
        <BlockView key={i} b={b} />
      ))}
    </div>
  );
}

type Tool = { icon?: "list" | "link"; text?: string; label: string; key?: string; run: Edit };
type Edit = (s: string, from: number, to: number) => { s: string; from: number; to: number };

/** Put `before` and `after` around the selection (or a placeholder word). */
const wrap =
  (before: string, after: string, placeholder: string): Edit =>
  (s, from, to) => {
    const inner = s.slice(from, to) || placeholder;
    const out = s.slice(0, from) + before + inner + after + s.slice(to);
    return { s: out, from: from + before.length, to: from + before.length + inner.length };
  };

/** Start every selected line (or the current one) with `prefix`, numbered when it is "1. ". */
const lines =
  (prefix: string): Edit =>
  (s, from, to) => {
    const start = s.lastIndexOf("\n", from - 1) + 1;
    const endAt = s.indexOf("\n", to);
    const end = endAt < 0 ? s.length : endAt;
    const block = s.slice(start, end).split("\n");
    const done = block
      .map((l, i) => {
        const bare = l.replace(/^(#{1,6} |[-*] |\d+\. |> )/, "");
        return (prefix === "1. " ? `${i + 1}. ` : prefix) + bare;
      })
      .join("\n");
    const out = s.slice(0, start) + done + s.slice(end);
    return { s: out, from: start, to: start + done.length };
  };

function tools(): Tool[] {
  return [
    { text: "H", label: t("Heading"), run: lines("## ") },
    { text: "B", label: t("Bold"), key: "b", run: wrap("**", "**", t("bold text")) },
    { text: "I", label: t("Italic"), key: "i", run: wrap("*", "*", t("italic text")) },
    { icon: "list", label: t("Bulleted list"), run: lines("- ") },
    { text: "1.", label: t("Numbered list"), run: lines("1. ") },
    { text: "❝", label: t("Quote"), run: lines("> ") },
    { icon: "link", label: t("Link"), key: "k", run: wrap("[", "](https://)", t("link text")) },
  ];
}

/**
 * The editor: toolbar, text and preview. The preview sits beside the text where the box is wide
 * enough (a container query in inputs.css, and always in full screen); a Write and Preview switch
 * takes its place where it is not.
 */
function Editor({
  value,
  onChange,
  common,
  wide,
  rows,
  onFull,
}: BaseInputProps & { wide: boolean; rows: number; onFull?: () => void }) {
  const area = useRef<HTMLTextAreaElement>(null);
  // A saved text opens as it reads (Preview); an empty one opens to be written.
  const [tab, setTab] = useState<"write" | "preview">(() =>
    String(value).trim() ? "preview" : "write",
  );
  const previewId = useId();
  const text = String(value);
  const apply = (run: Edit) => {
    const el = area.current;
    if (!el) return;
    const r = run(text, el.selectionStart, el.selectionEnd);
    onChange(r.s);
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(r.from, r.to);
    });
  };
  const all = tools();
  return (
    <div className={`mdfield${wide ? " wide" : ""}`} data-tab={tab}>
      <div className="mdbar" role="toolbar" aria-label={t("Formatting")}>
        {all.map((x) => (
          <button
            key={x.label}
            type="button"
            className="mdtool"
            title={x.key ? `${x.label} (Ctrl+${x.key.toUpperCase()})` : x.label}
            aria-label={x.label}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => apply(x.run)}
          >
            {x.icon ? <Icon name={x.icon} size={14} /> : <span aria-hidden="true">{x.text}</span>}
          </button>
        ))}
        <span className="grow" />
        {!wide && (
          <div className="mdtabs" role="tablist" aria-label={t("Write or preview")}>
            <button
              type="button"
              role="tab"
              aria-selected={tab === "write"}
              onClick={() => setTab("write")}
            >
              {t("Write")}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={tab === "preview"}
              aria-controls={previewId}
              onClick={() => setTab("preview")}
            >
              {t("Preview")}
            </button>
          </div>
        )}
        {onFull && (
          <button type="button" className="mdtool" title={t("Edit full screen")} onClick={onFull}>
            <Icon name="columns" size={14} />
            <span className="sr-only">{t("Edit full screen")}</span>
          </button>
        )}
      </div>
      <div className="mdpanes">
        <textarea
          {...common}
          ref={area}
          rows={rows}
          value={text}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => {
            const k = (e.ctrlKey || e.metaKey) && all.find((x) => x.key === e.key.toLowerCase());
            if (k) {
              e.preventDefault();
              apply(k.run);
            } else common.onKeyDown?.(e);
          }}
        />
        {/* A click in the text being read starts writing (a link still opens). */}
        <div
          className="mdpreview"
          id={previewId}
          aria-label={t("Preview")}
          role="region"
          onClick={(e) => {
            if (tab !== "preview" || (e.target as Element).closest("a")) return;
            setTab("write");
            requestAnimationFrame(() => area.current?.focus());
          }}
        >
          <MarkdownView text={text} />
        </div>
      </div>
    </div>
  );
}

/** A Markdown field: rendered when read only, the editor otherwise. */
export function MarkdownField(p: BaseInputProps) {
  const [full, setFull] = useState(false);
  if (p.readOnly) return <MarkdownView text={String(p.value)} className="mdread" />;
  if (p.compact)
    return (
      <textarea
        {...p.common}
        rows={2}
        value={String(p.value)}
        onChange={(e) => p.onChange(e.target.value)}
      />
    );
  return (
    <>
      <Editor {...p} wide={false} rows={8} onFull={() => setFull(true)} />
      {full && (
        <Dialog
          size="lg"
          className="mdfull"
          title={fieldTitle(p.field)}
          onClose={() => setFull(false)}
          footer={
            <button type="button" className="btn primary" onClick={() => setFull(false)}>
              {t("Done")}
            </button>
          }
        >
          <Editor {...p} common={{ ...p.common, id: undefined, autoFocus: true }} wide rows={24} />
        </Dialog>
      )}
    </>
  );
}
