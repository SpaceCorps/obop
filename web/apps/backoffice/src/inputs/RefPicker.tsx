// The picker of a ref field (plan 2.7 item 1): a combobox over the target model's records. The
// list floats in the top layer and flips above the input when there is no room below; exactly one
// option is highlighted. It matches every search field of the target and shows a grey second line;
// records with `active = false` are hidden unless one is the current value, and `ref_filter` (or
// the shared-scope rule of form.ts) narrows what is offered. A target of more than 200 records is
// searched on the server. 'Create "x"' makes a record from the typed name; "Create and edit…"
// opens the target's full form above and picks what it saved. Picking applies `fill_from`.
import {
  lazy,
  Suspense,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { Dialog, inSentence, t } from "@obop/ui";
import {
  ACTION_ONLY_MODELS,
  search,
  type FieldMeta,
  type ModelMeta,
  type Row,
  type ServerFilter,
} from "../api.ts";
import { labelField, useData, useRows } from "../data.tsx";
import { toEdit, type Edit } from "../edit.ts";
import { visibleFields } from "../format.ts";
import { fieldTitle, modelTitle } from "../labels.ts";
import {
  newDefaults,
  pickerSecondLine,
  searchFieldsOf,
  titleFieldsOf,
  titleOf,
  type PresentCtx,
} from "../presentation.ts";
import { serverMessage } from "../serverMessage.ts";
import { Icon } from "../ui.tsx";
import { fillsFor, fillValues, meets, refConditions, useFieldForm, type RefCond } from "./form.ts";
import { useAnchored } from "./popover.ts";

const RecordPanel = lazy(() =>
  import("../RecordPanel.tsx").then((m) => ({ default: m.RecordPanel })),
);

/** Above this many records the picker asks the server instead of filtering in the browser. */
export const SERVER_SEARCH_ABOVE = 200;
const SHOWN = 50;
const SERVER_LIMIT = 20;

export type RefPickerProps = {
  field: FieldMeta;
  value: Edit;
  onChange: (v: Edit) => void;
  readOnly?: boolean;
  id?: string;
  autoFocus?: boolean;
  invalid?: boolean;
  describedBy?: string;
  onKeyDown?: (e: KeyboardEvent) => void;
  compact?: boolean;
  /** The record's currency, for the amounts that picking fills in. */
  currency?: string;
};

type Item =
  | { kind: "none" }
  | { kind: "rec"; id: string; title: string; second: string }
  | { kind: "create"; text: string }
  | { kind: "edit"; text: string };

/** Lower case without accents, for matching what was typed. */
const fold = (s: string) =>
  s
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase();

/** The text a record is found by: its title, its search fields and its second line. */
function haystack(target: ModelMeta, row: Row, title: string, second: string): string {
  const parts = [title, second];
  for (const n of searchFieldsOf(target)) {
    const v = row[n];
    if (typeof v === "string" || typeof v === "number") parts.push(String(v));
  }
  return fold(parts.join(" "));
}

const toServer = (c: RefCond): ServerFilter => ({ field: c.field, op: c.op, value: c.value });

/** The next control of the form after `el` (the field after a picker), or null. */
function nextFocusable(el: HTMLElement | null): HTMLElement | null {
  const form = el?.closest("form");
  if (!el || !form) return null;
  const all = [
    ...form.querySelectorAll<HTMLElement>(
      "input:not([type=hidden]):not([disabled]), select:not([disabled]), textarea:not([disabled])",
    ),
  ].filter((x) => !x.closest(".refpicker")?.contains(el) || x === el);
  const at = all.indexOf(el);
  return at >= 0 ? (all[at + 1] ?? null) : null;
}

/** How many archived records match what was typed (they are not offered). */
function archivedMatches(
  rows: readonly Row[],
  words: string[],
  target: ModelMeta,
  ctx: PresentCtx,
) {
  if (!words.length) return 0;
  return rows.filter(
    (r) =>
      r.active === false &&
      words.every((w) => haystack(target, r, titleOf(target, r, ctx), "").includes(w)),
  ).length;
}

/** The text field that names a record of `model` (its title), if one does. */
function textTitleOf(model: ModelMeta): string | undefined {
  return titleFieldsOf(model)
    .map((n) => model.fields.find((f) => f.name === n))
    .find((f) => f?.kind === "text" && !f.computed && !f.sequence)?.name;
}

export function RefPicker(p: RefPickerProps) {
  const { field, value, onChange, readOnly } = p;
  const d = useData();
  const form = useFieldForm();
  const to = field.to ?? "";
  const info = d.models.get(to);
  const target = info?.model;
  const moduleId = info?.module.id ?? "";
  const targetTitle = target ? modelTitle(target, moduleId) : to.replace(/_/g, " ");
  const { rows, loaded, total } = useRows(to, !!target);
  const [query, setQuery] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [showAll, setShowAll] = useState(false);
  const [remote, setRemote] = useState<{ q: string; rows: Row[] } | null>(null);
  const [stacked, setStacked] = useState<Record<string, unknown> | null>(null);
  const [making, setMaking] = useState(false);
  const listId = useId();
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const current = String(value);
  const q = (query ?? "").trim();

  const ctx: PresentCtx = useMemo(
    () => ({
      refText: d.refText,
      rowOf: (m, id) => d.store.cached(m)?.find((r) => r.id === id),
      modelOf: (m) => d.models.get(m)?.model,
      moduleId,
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [d.refText, d.store, d.models, moduleId, d.version],
  );
  const rowById = (id: string): Row | undefined =>
    rows.find((r) => r.id === id) ??
    remote?.rows.find((r) => r.id === id) ??
    // A record made a moment ago is in the store before this render has its rows.
    d.store.cached(to)?.find((r) => r.id === id);
  const titleById = (id: string): string => {
    const r = rowById(id);
    return r && target ? titleOf(target, r, ctx) : d.refText(to, id);
  };

  const peek = (name: string) => form?.peek(name);
  const scoped = refConditions(form?.model, field, target, peek);
  const conds = showAll && scoped.scope.length ? [] : scoped.conds;
  const hasActive = !!target?.fields.some((f) => f.name === "active" && f.kind === "bool");
  const serverMode = loaded && (total > SERVER_SEARCH_ABOVE || total > rows.length);

  // A big model is searched on the server, a little after the last key.
  useEffect(() => {
    if (!open || !serverMode || !target || !q) return;
    let live = true;
    const timer = setTimeout(() => {
      const extra = [
        ...conds.map(toServer),
        ...(hasActive ? [{ field: "active", op: "ne", value: false } as ServerFilter] : []),
      ];
      const fields = searchFieldsOf(target).slice(0, 3);
      void Promise.all(
        fields.map((f) =>
          search(d.token, to, {
            filter: [{ field: f, op: "contains", value: q }, ...extra],
            limit: SERVER_LIMIT,
          }),
        ),
      ).then((answers) => {
        if (!live) return;
        const seen = new Set<string>();
        const out: Row[] = [];
        for (const a of answers)
          if (!("error" in a))
            for (const r of a.rows)
              if (!seen.has(r.id)) {
                seen.add(r.id);
                out.push(r);
              }
        setRemote({ q, rows: out.slice(0, SERVER_LIMIT) });
      });
    }, 200);
    return () => {
      live = false;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, serverMode, q, to, JSON.stringify(conds), hasActive]);

  const options = useMemo(() => {
    if (!target) return [];
    const words = fold(q).split(/\s+/).filter(Boolean);
    const pool = serverMode && q ? (remote?.q === q ? remote.rows : []) : rows;
    const out: { id: string; title: string; second: string; rank: number }[] = [];
    for (const r of pool) {
      if (hasActive && r.active === false && r.id !== current) continue;
      if (!meets(r, conds) && r.id !== current) continue;
      const title = titleOf(target, r, ctx);
      const second = pickerSecondLine(target, r, ctx);
      if (words.length && !(serverMode && q)) {
        const hay = haystack(target, r, title, second);
        if (!words.every((w) => hay.includes(w))) continue;
      }
      const ft = fold(title);
      const rank = !words.length ? 0 : ft.startsWith(fold(q)) ? 0 : ft.includes(words[0]!) ? 1 : 2;
      out.push({ id: r.id, title, second, rank });
    }
    if (words.length) out.sort((a, b) => a.rank - b.rank || a.title.localeCompare(b.title));
    return out.slice(0, SHOWN);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, remote, q, target, ctx, current, JSON.stringify(conds), serverMode, hasActive, d.lang]);

  const archived = useMemo(
    () =>
      hasActive && target && !serverMode
        ? archivedMatches(rows, fold(q).split(/\s+/).filter(Boolean), target, ctx)
        : 0,
    [hasActive, target, serverMode, rows, q, ctx, d.lang],
  );

  const titleFields = target
    ? titleFieldsOf(target).map((n) => target.fields.find((f) => f.name === n))
    : [];
  // A record named by one ref and nothing else (a vendor is its contact): the typed name is the
  // name of that record, made or found first, never another text field of this one ("Our customer
  // number"). A title of several parts (a stock item is a product at a location) makes nothing.
  const viaRef = (() => {
    const f = titleFields[0];
    if (!target || titleFields.length !== 1 || f?.kind !== "ref" || !f.to) return undefined;
    const other = d.models.get(f.to)?.model;
    const name = other ? textTitleOf(other) : undefined;
    return other?.can_create && name ? { field: f.name, to: f.to, name } : undefined;
  })();
  // A boolean for the list below: the object above is new on every render, and a list that
  // changes on every render would reset the highlighted option (Enter would pick nothing).
  const makesRef = !!viaRef;
  const labelName = target
    ? (titleFields.find((f) => f?.kind === "text" && !f.computed && !f.sequence)?.name ??
      (titleFields[0]?.kind === "ref" ? undefined : labelField(target)?.name))
    : undefined;
  const canCreate = !!target?.can_create && !ACTION_ONLY_MODELS.has(to) && !readOnly;
  const exact = options.some((o) => fold(o.title) === fold(q));
  const items: Item[] = useMemo(() => {
    const list: Item[] = [];
    if (!field.required && !q) list.push({ kind: "none" });
    for (const o of options) list.push({ kind: "rec", ...o });
    if (canCreate && q && (labelName || makesRef) && !exact) list.push({ kind: "create", text: q });
    if (canCreate) list.push({ kind: "edit", text: q });
    return list;
  }, [options, q, canCreate, labelName, makesRef, exact, field.required]);

  // One highlighted option: the current value when it is listed, else the first record.
  useEffect(() => {
    if (!open) return;
    const at = items.findIndex((it) => it.kind === "rec" && it.id === current);
    const first = items.findIndex((it) => it.kind === "rec");
    // Typing that matches nothing highlights nothing: Enter never creates a record by surprise;
    // "Create “x”" takes a click or the arrow keys.
    setActive(q ? first : at >= 0 ? at : Math.max(0, first));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, q, items.length]);

  useEffect(() => {
    if (!open) return;
    const on = (e: MouseEvent) => {
      if (box.current && !e.composedPath().includes(box.current)) close();
    };
    addEventListener("mousedown", on);
    return () => removeEventListener("mousedown", on);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    pop.current
      ?.querySelector<HTMLElement>(`[data-index="${active}"]`)
      ?.scrollIntoView?.({ block: "nearest" });
  }, [active, open]);

  useAnchored(open, input, pop, { maxHeight: 320, minWidth: 240 });

  if (!target)
    return (
      <input
        id={p.id}
        value={current}
        readOnly={readOnly}
        onChange={(e) => onChange(e.target.value)}
        placeholder={t("{model} id", { model: to })}
      />
    );

  function close() {
    setOpen(false);
    setQuery(null);
    setShowAll(false);
  }

  function pick(rid: string) {
    if (form && rid !== current) {
      const fills = fillsFor(form.model, field, target);
      if (fills.length) {
        const values = fillValues(fills, rowById(current), rowById(rid), form.peek, (f, v) =>
          toEdit(f, v, p.currency),
        );
        onChange(rid);
        if (Object.keys(values).length) form.fill(values);
      } else onChange(rid);
    } else onChange(rid);
    close();
  }

  /** What a new record starts with: the typed name and the scope of the form. */
  function startValues(text: string): Record<string, unknown> {
    const out: Record<string, unknown> = { ...newDefaults(target!) };
    if (labelName && text) out[labelName] = text;
    for (const c of scoped.conds)
      if (c.op === "eq" && target!.fields.some((f) => f.name === c.field)) out[c.field] = c.value;
    return out;
  }

  /** The record a ref-named model points at: the one of that exact name, else a new one. */
  async function refFor(text: string): Promise<string | null> {
    if (!viaRef) return null;
    const found = await search(d.token, viaRef.to, {
      filter: [{ field: viaRef.name, op: "eq", value: text }],
      limit: 2,
    });
    if (!("error" in found) && found.rows.length === 1) return found.rows[0]!.id;
    const made = await d.run(`${viaRef.to}.create`, { [viaRef.name]: text });
    if (made.kind === "ok") return String((made.value as Row).id ?? "") || null;
    if (made.kind === "error") d.notify(serverMessage(made.message), "error");
    return null;
  }

  async function quickCreate(text: string) {
    if ((!labelName && !viaRef) || making) return;
    setMaking(true);
    const start = startValues(text);
    if (viaRef) {
      const rid = await refFor(text);
      if (!rid) {
        setMaking(false);
        openStacked(text);
        return;
      }
      start[viaRef.field] = rid;
    }
    const r = await d.run(`${to}.create`, start);
    setMaking(false);
    if (r.kind === "ok") {
      const id = String((r.value as Row).id ?? "");
      d.notify(t("{model} “{name}” created", { model: targetTitle, name: text }));
      if (id) pick(id);
    } else if (r.kind === "approval") {
      d.notify(
        t("Approval needed: the new record was proposed. A human must approve it first."),
        "info",
      );
      close();
    } else {
      // Something else is required: the full form takes it from here.
      d.notify(serverMessage(r.message), "error");
      setStacked(start);
      setOpen(false);
    }
  }

  function openStacked(text: string) {
    setStacked(startValues(text));
    setOpen(false);
  }

  function choose(it: Item | undefined) {
    if (!it) return;
    if (it.kind === "none") pick("");
    else if (it.kind === "rec") pick(it.id);
    else if (it.kind === "create") void quickCreate(it.text);
    else openStacked(it.text);
  }

  const keys = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (!open) setOpen(true);
      else setActive((a) => Math.min(a + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter" && open) {
      e.preventDefault();
      e.stopPropagation();
      choose(items[active]);
    } else if (e.key === "Escape" && open) {
      // The list closes; the record panel around it stays open.
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "Tab") {
      close();
      // A list cell's editor saves and moves on with Tab (InlineEdit): it must see the key too.
      p.onKeyDown?.(e);
    } else p.onKeyDown?.(e);
  };

  const shown = open && query !== null ? query : current ? titleById(current) : "";
  const fieldName = fieldTitle(field);
  const optionId = (i: number) => `${listId}-${i}`;
  const scopeNames = scoped.scope.map((f) => inSentence(fieldTitle(f))).join(", ");

  return (
    <div className={`refpicker${p.compact ? " compact" : ""}`} ref={box}>
      <input
        ref={input}
        id={p.id}
        role="combobox"
        aria-expanded={open}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={open && items[active] ? optionId(active) : undefined}
        aria-invalid={p.invalid || undefined}
        aria-describedby={p.describedBy}
        aria-required={field.required || undefined}
        aria-busy={making || undefined}
        autoFocus={p.autoFocus}
        autoComplete="off"
        spellCheck={false}
        readOnly={readOnly}
        value={shown}
        placeholder={
          loaded ? t("Search {model}…", { model: inSentence(targetTitle) }) : t("Loading…")
        }
        onFocus={(e) => {
          if (readOnly) return;
          setOpen(true);
          e.currentTarget.select();
        }}
        onClick={() => !readOnly && setOpen(true)}
        onBlur={(e) => {
          if (!box.current?.contains(e.relatedTarget as Node | null)) close();
        }}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
        }}
        onKeyDown={keys}
      />
      {!readOnly && current && (
        <button
          type="button"
          className="icon-btn clear"
          tabIndex={-1}
          aria-label={t("Clear {field}", { field: fieldName })}
          onClick={() => pick("")}
        >
          <Icon name="close" size={12} />
        </button>
      )}
      {open && (
        <div ref={pop} className="rp-pop" popover="manual" data-side="bottom">
          <ul id={listId} role="listbox" aria-label={t("{field} options", { field: fieldName })}>
            {options.length === 0 && (
              <li className="rp-empty" role="presentation">
                {!loaded || (serverMode && q && remote?.q !== q) ? t("Searching…") : t("No match")}
              </li>
            )}
            {archived > 0 && (
              <li className="rp-empty" role="presentation">
                {archived === 1
                  ? t("1 archived match is not offered")
                  : t("{count} archived matches are not offered", { count: archived })}
              </li>
            )}
            {items.map((it, i) => {
              const props = {
                id: optionId(i),
                "data-index": i,
                role: "option",
                "aria-selected": i === active,
                onMouseEnter: () => setActive(i),
                onMouseDown: (e: ReactMouseEvent) => {
                  e.preventDefault();
                  choose(it);
                },
              } as const;
              if (it.kind === "none")
                return (
                  <li key="none" {...props} className="rp-none">
                    {t("None")}
                  </li>
                );
              if (it.kind === "rec")
                return (
                  <li key={it.id} {...props} data-current={it.id === current || undefined}>
                    <span className="rp-main">
                      <span className="rp-title">{it.title}</span>
                      {it.second && <span className="rp-second">{it.second}</span>}
                    </span>
                    {it.id === current && <Icon name="check" size={14} />}
                  </li>
                );
              if (it.kind === "create")
                return (
                  <li key="create" {...props} className="rp-action">
                    <Icon name="plus" size={14} />
                    <span>{t("Create “{name}”", { name: it.text })}</span>
                  </li>
                );
              return (
                <li key="edit" {...props} className="rp-action">
                  <Icon name="edit" size={14} />
                  <span>
                    {it.text
                      ? t("Create and edit…")
                      : t("Create a new {model}…", { model: inSentence(targetTitle) })}
                  </span>
                </li>
              );
            })}
          </ul>
          {scoped.scope.length > 0 && (
            <div className="rp-foot">
              <span>
                {showAll
                  ? t("Showing every {model}", { model: inSentence(targetTitle) })
                  : t("Only those of the same {fields}", { fields: scopeNames })}
              </span>
              <button
                type="button"
                className="linkbtn"
                tabIndex={-1}
                onMouseDown={(e) => {
                  e.preventDefault();
                  setShowAll((s) => !s);
                }}
              >
                {showAll ? t("Narrow") : t("Show all")}
              </button>
            </div>
          )}
        </div>
      )}
      {stacked && (
        <Dialog
          bare
          size="md"
          className="rp-stack"
          title={t("New {model}", { model: inSentence(targetTitle) })}
          onClose={() => setStacked(null)}
        >
          <Suspense fallback={<p className="hint">{t("Loading…")}</p>}>
            <RecordPanel
              model={target}
              fields={visibleFields(target.fields)}
              row={null}
              defaults={stacked}
              withheld={[]}
              title={targetTitle}
              onClose={() => setStacked(null)}
              onSaved={(id) => {
                setStacked(null);
                pick(id);
                // Focus moves on to the next field of the form, not back into this list.
                const next = nextFocusable(input.current);
                (next ?? input.current)?.focus();
              }}
            />
          </Suspense>
        </Dialog>
      )}
    </div>
  );
}
