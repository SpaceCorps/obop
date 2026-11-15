import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { inSentence, t, useI18n, type IconName, type MenuItem } from "@obop/ui";
import type { FieldMeta, ModelMeta } from "./api.ts";
import { useData } from "./data.tsx";
import type { Favorite } from "./favorites.ts";
import { FilterPanel, metaOr, useQuickFilters } from "./FilterPanel.tsx";
import { optionText, quickLabel, searchTargets, searchableField } from "./filters.ts";
import { optionLabel } from "./format.ts";
import { fieldTitleByName, fieldTitle } from "./labels.ts";
import { PHONE_QUERY, useMedia, useScrollLock } from "./media.ts";
import { describeCond, type Cond, type Lookup, type ViewState } from "./query.ts";
import { Button, Icon, Menu, Segmented, useDismiss } from "./ui.tsx";

export type ViewDef = { id: string; label: string; icon: IconName };

type Props = {
  appName: string;
  /** The model's name for people, singular ("Sale order"): labels and the search's placeholder. */
  title: string;
  /** The page's heading; the plural when it is known ("Sale orders"). Default: `title`. */
  heading?: string;
  model: string;
  /** The model's metadata: generated filters and search suggestions read it. */
  meta?: ModelMeta;
  fields: FieldMeta[];
  canCreate: boolean;
  onNew: () => void;
  /** Import records from a CSV file (D70); the button shows when it is given. */
  onImport?: () => void;
  /** Export what the search matches as CSV; offered in the phone's ⋯ menu when given. */
  onExport?: () => void;
  state: ViewState;
  setState: (s: ViewState) => void;
  lookup: Lookup;
  views: ViewDef[];
  view: string;
  setView: (v: string) => void;
  applyFavorite: (f: Favorite) => void;
  count: number;
  extra?: ReactNode;
  /**
   * The typed text, 250 ms after the last key, before it becomes a chip: the page narrows the
   * records to it at once. "" when the box is empty again.
   */
  onLive?: (text: string) => void;
  /** The signed-in user name, for the "Mine" filter. */
  user?: string;
};

type Opt = { key: string; label: string; cond: Cond; group: "all" | "match" | "field" };

/** The most option and record suggestions the search box shows. */
const MAX_MATCHES = 6;

/**
 * Puts `layer` (a `popover="manual"` element) in the top layer while `open`, under `anchor`, so
 * nothing it sits in can clip it (B2: the filter panel was cut off under a short list on a tablet).
 * It keeps following the anchor as the page scrolls. On a phone CSS makes the layer a bottom
 * sheet and ignores the position.
 */
function useTopLayer(
  layer: RefObject<HTMLElement | null>,
  anchor: RefObject<HTMLElement | null>,
  open: boolean,
  matchWidth = false,
) {
  useLayoutEffect(() => {
    const el = layer.current;
    if (!open || !el) return;
    try {
      if (typeof el.showPopover === "function" && !el.matches(":popover-open")) el.showPopover();
    } catch {
      /* not connected, or no popover support: it stays a fixed box */
    }
    let frame = 0;
    const place = () => {
      frame = 0;
      const a = anchor.current?.getBoundingClientRect();
      if (!a) return;
      const vw = document.documentElement.clientWidth;
      const vh = window.innerHeight;
      const edge = 16;
      const width = matchWidth ? a.width : Math.min(el.offsetWidth || a.width, vw - 2 * edge);
      const rtl = getComputedStyle(el).direction === "rtl";
      const start = rtl ? a.right - width : a.left;
      const left = Math.max(edge, Math.min(start, vw - width - edge));
      const top = a.bottom + 6;
      el.style.setProperty("--at-left", `${Math.round(left)}px`);
      el.style.setProperty("--at-top", `${Math.round(top)}px`);
      el.style.setProperty("--at-width", `${Math.round(a.width)}px`);
      el.style.setProperty("--at-room", `${Math.max(160, Math.round(vh - top - edge))}px`);
    };
    place();
    const later = () => {
      if (!frame) frame = requestAnimationFrame(place);
    };
    addEventListener("resize", later);
    addEventListener("scroll", later, true);
    const ro = typeof ResizeObserver === "function" ? new ResizeObserver(later) : undefined;
    ro?.observe(el);
    if (anchor.current) ro?.observe(anchor.current);
    return () => {
      removeEventListener("resize", later);
      removeEventListener("scroll", later, true);
      ro?.disconnect();
      if (frame) cancelAnimationFrame(frame);
      try {
        if (el.isConnected && el.matches(":popover-open")) el.hidePopover();
      } catch {
        /* already gone */
      }
    };
  }, [layer, anchor, open, matchWidth]);
}

/**
 * The model page's bar: the title with New (and Import), the faceted search box with live search
 * and suggestions, the Filters / Group by / Favorites panel in the top layer, and the view
 * switcher. On a phone: one row with the title, the view menu and ⋯ (Import, Export), the search
 * row, and New as a floating button; the panel is a bottom sheet.
 */
export function ControlPanel(p: Props) {
  const { state, setState, fields, lookup } = p;
  const d = useData();
  const { lang } = useI18n();
  const meta = useMemo(() => metaOr(p.meta, p.model, fields), [p.meta, p.model, fields]);
  const quick = useQuickFilters(meta, fields, p.user);
  const [text, setText] = useState("");
  const [open, setOpen] = useState(false);
  const [suggest, setSuggest] = useState(false);
  const [active, setActive] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const layer = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const listId = useId();
  const panelId = useId();
  useDismiss(root, open, () => setOpen(false));
  // On a phone the filter panel is a bottom sheet: the list behind it stays still.
  const phone = useMedia(PHONE_QUERY);
  useScrollLock(open && phone);
  useTopLayer(layer, box, open);

  // Live search: the text narrows the records 250 ms after the last key; Enter makes it a chip.
  const onLive = p.onLive;
  const liveRef = useRef(onLive);
  liveRef.current = onLive;
  useEffect(() => {
    if (!liveRef.current) return;
    const q = text.trim();
    const id = setTimeout(() => liveRef.current?.(q), q ? 250 : 0);
    return () => clearTimeout(id);
  }, [text]);

  const targets = useMemo(() => searchTargets(meta, fields), [meta, fields]);
  const options = useMemo(() => {
    const q = text.trim();
    if (!q) return [];
    const needle = q.toLocaleLowerCase();
    const out: Opt[] = [
      {
        key: "all",
        label: t("Search everywhere for: {text}", { text: q }),
        cond: { op: "search", v: q },
        group: "all",
      },
    ];
    // Options whose name fits ("State: Draft"), then records of the references ("Customer:
    // Baumann Bau GmbH"): one tap filters on exactly that.
    const matches: Opt[] = [];
    for (const f of fields) {
      if (f.kind !== "select" || !searchableField(f)) continue;
      for (const o of f.options ?? [])
        if (optionLabel(o, f).toLocaleLowerCase().includes(needle))
          matches.push({
            key: `o:${f.name}:${o}`,
            label: optionText(f, o),
            cond: { op: "is", f: f.name, v: [o] },
            group: "match",
          });
    }
    const refs = fields.filter((f) => f.kind === "ref" && f.to && searchableField(f)).slice(0, 3);
    for (const f of refs) {
      for (const row of d.store.peek(f.to!) ?? []) {
        const label = d.recordLabel(f.to!, row);
        if (!label.toLocaleLowerCase().includes(needle)) continue;
        matches.push({
          key: `r:${f.name}:${row.id}`,
          label: t("{field}: {values}", { field: fieldTitle(f), values: label }),
          cond: { op: "is", f: f.name, v: [row.id] },
          group: "match",
        });
        if (matches.length >= MAX_MATCHES * 2) break;
      }
    }
    out.push(...matches.slice(0, MAX_MATCHES));
    for (const f of targets)
      out.push({
        key: f.name,
        label: t("Search {field} for: {text}", { field: fieldTitle(f), text: q }),
        cond: { op: "contains", f: f.name, v: q },
        group: "field",
      });
    return out;
    // d.version: the rows of a reference arrived; lang: the labels are translated.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text, fields, targets, lang, d.version]);
  const showList = suggest && options.length > 0;
  useTopLayer(list, box, showList, true);

  const add = (c: Cond) => {
    setState({ ...state, facets: [...state.facets, c] });
    setText("");
    onLive?.("");
    setSuggest(false);
  };
  const removeAt = (i: number) =>
    setState({ ...state, facets: state.facets.filter((_, j) => j !== i) });
  const clearText = () => {
    setText("");
    onLive?.("");
    setSuggest(false);
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown" && e.altKey) {
      e.preventDefault();
      setOpen(true);
    } else if (e.key === "ArrowDown" && options.length) {
      e.preventDefault();
      setActive((a) => (suggest ? Math.min(a + 1, options.length - 1) : 0));
      setSuggest(true);
    } else if (e.key === "ArrowUp" && options.length) {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter" && text.trim()) {
      e.preventDefault();
      add((suggest ? (options[active] ?? options[0]) : options[0])!.cond);
    } else if (e.key === "Backspace" && !text && state.facets.length) {
      removeAt(state.facets.length - 1);
    } else if (e.key === "Escape") {
      if (suggest || text) {
        e.stopPropagation();
        clearText();
      } else if (open) {
        e.stopPropagation();
        setOpen(false);
      }
    }
  };

  const chipText = (c: Cond) => quickLabel(c, quick) ?? describeCond(c, fields, lookup);
  const filtersOn = state.facets.length + state.group.length;
  const heading = p.heading ?? p.title;

  const more: MenuItem[] = [
    ...(p.onImport
      ? [{ label: t("Import from a CSV file"), icon: "upload" as const, onSelect: p.onImport }]
      : []),
    ...(p.onExport
      ? [{ label: t("Export as CSV"), icon: "download" as const, onSelect: p.onExport }]
      : []),
  ];
  const current = p.views.find((v) => v.id === p.view) ?? p.views[0];

  return (
    <div
      className="controlpanel"
      role="search"
      aria-label={t("{model} search", { model: p.title })}
    >
      <div className="cp-top">
        <div className="crumbs">
          <span>{p.appName}</span>
          <Icon name="chevron-right" size={12} />
          <strong>{heading}</strong>
        </div>
        {phone ? (
          <div className="cp-phone">
            {p.views.length > 1 && current && (
              <label className="cp-viewmenu">
                <Icon name={current.icon} size={16} />
                <select
                  aria-label={t("View")}
                  value={p.view}
                  onChange={(e) => p.setView(e.target.value)}
                >
                  {p.views.map((v) => (
                    <option key={v.id} value={v.id}>
                      {t(v.label)}
                    </option>
                  ))}
                </select>
                <Icon name="chevron-down" size={14} />
              </label>
            )}
            {more.length > 0 && (
              <Menu
                trigger={<Icon name="more" size={18} />}
                ariaLabel={t("More actions")}
                triggerClassName="icon-btn cp-more"
                align="end"
                items={more}
              />
            )}
          </div>
        ) : (
          <div className="cp-actions">
            {p.onImport && (
              <Button icon="upload" aria-label={t("Import")} onClick={p.onImport}>
                <span className="cp-label">{t("Import")}</span>
              </Button>
            )}
            {p.canCreate && (
              <Button variant="primary" icon="plus" onClick={p.onNew}>
                {t("New")}
              </Button>
            )}
          </div>
        )}
      </div>
      <div className="cp-bottom">
        <div className="searchwrap" ref={root}>
          <div
            ref={box}
            className={`searchbox${open ? " open" : ""}`}
            onClick={() => input.current?.focus()}
          >
            <Icon name="search" size={15} />
            <ul className="facets" aria-label={t("Active filters")}>
              {state.facets.map((c, i) => (
                <li key={`${i}-${JSON.stringify(c)}`} className="facet">
                  <span>{chipText(c)}</span>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      removeAt(i);
                    }}
                    aria-label={t("Remove filter {filter}", { filter: chipText(c) })}
                  >
                    <Icon name="close" size={11} />
                  </button>
                </li>
              ))}
              {state.group.map((g) => (
                <li key={`g-${g}`} className="facet group">
                  <span>{t("Group: {field}", { field: fieldTitleByName(g, fields) })}</span>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      setState({ ...state, group: state.group.filter((x) => x !== g) });
                    }}
                    aria-label={t("Remove grouping by {field}", {
                      field: fieldTitleByName(g, fields),
                    })}
                  >
                    <Icon name="close" size={11} />
                  </button>
                </li>
              ))}
            </ul>
            <input
              ref={input}
              role="combobox"
              aria-expanded={showList}
              aria-controls={listId}
              aria-autocomplete="list"
              aria-label={t("Search {model}", { model: inSentence(p.title) })}
              aria-activedescendant={showList ? `${listId}-${active}` : undefined}
              placeholder={filtersOn ? "" : t("Search {model}…", { model: inSentence(heading) })}
              value={text}
              enterKeyHint="search"
              onChange={(e) => {
                setText(e.target.value);
                setSuggest(true);
                setActive(0);
              }}
              onKeyDown={onKey}
              onBlur={() => setSuggest(false)}
            />
            {text && (
              <button
                type="button"
                className="icon-btn cp-clear"
                aria-label={t("Clear the search text")}
                onClick={(e) => {
                  e.stopPropagation();
                  clearText();
                  input.current?.focus();
                }}
              >
                <Icon name="close" size={13} />
              </button>
            )}
            <button
              type="button"
              className="icon-btn caretbtn"
              aria-label={t("Filters, group by and favorites")}
              aria-haspopup="true"
              aria-expanded={open}
              aria-controls={panelId}
              onClick={(e) => {
                e.stopPropagation();
                setOpen(!open);
              }}
            >
              <Icon name="filter" size={15} />
            </button>
          </div>
          {showList && (
            <ul
              ref={list}
              popover="manual"
              id={listId}
              role="listbox"
              className="suggest panel"
              aria-label={t("Search options")}
            >
              {options.map((o, i) => (
                <li
                  key={o.key}
                  id={`${listId}-${i}`}
                  role="option"
                  aria-selected={i === active}
                  className={i > 0 && options[i - 1]!.group !== o.group ? "sg-first" : undefined}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    add(o.cond);
                  }}
                  onMouseEnter={() => setActive(i)}
                >
                  {o.group === "match" && <Icon name="filter" size={13} />}
                  {o.group === "field" && <Icon name="search" size={13} />}
                  {o.group === "all" && <Icon name="search" size={13} />}
                  <span>{o.label}</span>
                </li>
              ))}
            </ul>
          )}
          {open && (
            <div ref={layer} popover="manual" className="fp-layer">
              {/* Inside the dismiss root, so a tap on the backdrop closes the sheet and never
                  reaches the list underneath. Shown on a phone only (controlpanel.css). */}
              <div className="fp-scrim" aria-hidden="true" onClick={() => setOpen(false)} />
              <div id={panelId} className="fp-wrap panel">
                <div className="fp-head">
                  <strong>{t("Filters")}</strong>
                  <Button size="sm" onClick={() => setOpen(false)}>
                    {t("Done")}
                  </Button>
                </div>
                <FilterPanel
                  model={p.model}
                  meta={meta}
                  fields={fields}
                  state={state}
                  view={p.view}
                  setState={setState}
                  user={p.user}
                  quick={quick}
                  applyFavorite={(f) => {
                    p.applyFavorite(f);
                    setOpen(false);
                  }}
                />
              </div>
            </div>
          )}
        </div>
        {p.extra}
        <span className="grow" />
        <span className="count" aria-live="polite">
          {t("{count, plural, one {# record} other {# records}}", { count: p.count })}
        </span>
        {!phone && p.views.length > 1 && (
          <Segmented
            mode="toggle"
            label={t("View")}
            value={p.view}
            onChange={p.setView}
            options={p.views.map((v) => ({ value: v.id, label: t(v.label), icon: v.icon }))}
          />
        )}
      </div>
      {phone && p.canCreate && (
        <button type="button" className="cp-fab" aria-label={t("New")} onClick={p.onNew}>
          <Icon name="plus" size={24} />
        </button>
      )}
    </div>
  );
}
