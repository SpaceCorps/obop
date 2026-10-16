import { useId, useMemo, useRef, useState, type ReactNode } from "react";
import { inSentence, t, type IconName } from "@obop/ui";
import type { FieldMeta } from "./api.ts";
import type { Favorite } from "./favorites.ts";
import { FilterPanel } from "./FilterPanel.tsx";
import { fieldTitleByName, fieldTitle } from "./labels.ts";
import { describeCond, type Cond, type Lookup, type ViewState } from "./query.ts";
import { Button, Icon, Segmented, useDismiss } from "./ui.tsx";

export type ViewDef = { id: string; label: string; icon: IconName };

type Props = {
  appName: string;
  title: string;
  model: string;
  fields: FieldMeta[];
  canCreate: boolean;
  onNew: () => void;
  state: ViewState;
  setState: (s: ViewState) => void;
  lookup: Lookup;
  views: ViewDef[];
  view: string;
  setView: (v: string) => void;
  applyFavorite: (f: Favorite) => void;
  count: number;
  extra?: ReactNode;
};

/** Breadcrumbs, the faceted search box with its Filters/Group By/Favorites panel, and the view switcher. */
export function ControlPanel(p: Props) {
  const { state, setState, fields, lookup } = p;
  const [text, setText] = useState("");
  const [open, setOpen] = useState(false);
  const [suggest, setSuggest] = useState(false);
  const [active, setActive] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const listId = useId();
  const panelId = useId();
  useDismiss(root, open, () => setOpen(false));

  const searchable = fields.filter((f) => ["text", "longtext", "select", "ref"].includes(f.kind));
  const options = useMemo(() => {
    const q = text.trim();
    if (!q) return [];
    const all: { key: string; label: string; cond: Cond }[] = [
      {
        key: "all",
        label: t("Search everywhere for: {text}", { text: q }),
        cond: { op: "search", v: q },
      },
    ];
    for (const f of searchable)
      all.push({
        key: f.name,
        label: t("Search {field} for: {text}", { field: fieldTitle(f), text: q }),
        cond: { op: "contains", f: f.name, v: q },
      });
    return all;
  }, [text, searchable]);

  const add = (c: Cond) => {
    setState({ ...state, facets: [...state.facets, c] });
    setText("");
    setSuggest(false);
  };
  const removeAt = (i: number) =>
    setState({ ...state, facets: state.facets.filter((_, j) => j !== i) });

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown" && e.altKey) {
      e.preventDefault();
      setOpen(true);
    } else if (e.key === "ArrowDown" && options.length) {
      e.preventDefault();
      setSuggest(true);
      setActive((a) => Math.min(a + 1, options.length - 1));
    } else if (e.key === "ArrowUp" && options.length) {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter" && text.trim()) {
      e.preventDefault();
      add((suggest ? options[active] : options[0])!.cond);
    } else if (e.key === "Backspace" && !text && state.facets.length) {
      removeAt(state.facets.length - 1);
    } else if (e.key === "Escape") {
      if (suggest || text) {
        e.stopPropagation();
        setText("");
        setSuggest(false);
      } else if (open) {
        e.stopPropagation();
        setOpen(false);
      }
    }
  };

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
          <strong>{p.title}</strong>
        </div>
        {p.canCreate && (
          <Button variant="primary" icon="plus" onClick={p.onNew}>
            {t("New")}
          </Button>
        )}
      </div>
      <div className="cp-bottom">
        <div className="searchwrap" ref={root}>
          <div className={`searchbox${open ? " open" : ""}`} onClick={() => input.current?.focus()}>
            <Icon name="search" size={15} />
            <ul className="facets" aria-label={t("Active filters")}>
              {state.facets.map((c, i) => (
                <li key={`${i}-${JSON.stringify(c)}`} className="facet">
                  <span>{describeCond(c, fields, lookup)}</span>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      removeAt(i);
                    }}
                    aria-label={t("Remove filter {filter}", {
                      filter: describeCond(c, fields, lookup),
                    })}
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
              aria-expanded={suggest && options.length > 0}
              aria-controls={listId}
              aria-autocomplete="list"
              aria-label={t("Search {model}", { model: inSentence(p.title) })}
              aria-activedescendant={suggest && options.length ? `${listId}-${active}` : undefined}
              placeholder={
                state.facets.length + state.group.length
                  ? ""
                  : t("Search {model}…", { model: inSentence(p.title) })
              }
              value={text}
              onChange={(e) => {
                setText(e.target.value);
                setSuggest(true);
                setActive(0);
              }}
              onKeyDown={onKey}
              onBlur={() => setSuggest(false)}
            />
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
          {suggest && options.length > 0 && (
            <ul
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
                  onMouseDown={(e) => {
                    e.preventDefault();
                    add(o.cond);
                  }}
                  onMouseEnter={() => setActive(i)}
                >
                  {o.label}
                </li>
              ))}
            </ul>
          )}
          {open && (
            <div id={panelId} className="fp-wrap panel">
              <FilterPanel
                model={p.model}
                fields={fields}
                state={state}
                view={p.view}
                setState={setState}
                applyFavorite={(f) => {
                  p.applyFavorite(f);
                  setOpen(false);
                }}
              />
            </div>
          )}
        </div>
        {p.extra}
        <span className="grow" />
        <span className="count" aria-live="polite">
          {t("{count, plural, one {# record} other {# records}}", { count: p.count })}
        </span>
        <Segmented
          mode="toggle"
          label={t("View")}
          iconOnlyNarrow
          value={p.view}
          onChange={p.setView}
          options={p.views.map((v) => ({ value: v.id, label: t(v.label), icon: v.icon }))}
        />
      </div>
    </div>
  );
}
