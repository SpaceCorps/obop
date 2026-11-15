/**
 * The "Records of" picker of the tile form: a searchable list of models grouped by app, the
 * recently used first. Set-up and log models (a tax, an SLA, a security event, a translation) are
 * left out until the person searches, so the list is the records a dashboard is about.
 */
import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { compareText, t, useI18n } from "@obop/ui";
import type { ModelMeta } from "../api.ts";
import type { App } from "../Chrome.tsx";
import { useData } from "../data.tsx";
import { appTitle, modelTitle } from "../labels.ts";
import { childrenOf, indexModels, menuOf } from "../presentation.ts";
import { Icon } from "../ui.tsx";

export type ModelChoice = {
  model: ModelMeta;
  app: App;
  /** The model's name and its app's, in the person's language. */
  title: string;
  appName: string;
  /** Set-up or log data: offered only to a search. */
  quiet: boolean;
};

/** Logs, previews, policies and the parts of a record that are not records of their own. */
const QUIET =
  /(_event|_events|_log|_logs|_audit|_history|_register|_policy|_preview|_translation|_image|_use)$/;

/** Every model a dashboard can read, by app, with the quiet ones marked. */
export function useModelChoices(): ModelChoice[] {
  const d = useData();
  return useMemo(() => {
    const index = indexModels(d.modules);
    const lines = new Set<string>();
    for (const m of index.values())
      for (const c of childrenOf(m, index))
        if ((c.shape ?? "lines") === "lines") lines.add(c.model);
    const out: ModelChoice[] = [];
    for (const app of d.apps) {
      if (app.id === "customize" || app.id === "dashboards") continue;
      const menu = menuOf(app.module, index);
      const main = new Set([...menu.main, ...menu.reporting].map((m) => m.name));
      for (const model of app.module.models)
        out.push({
          model,
          app,
          title: modelTitle(model, app.module.id),
          appName: app.name || appTitle(app.module),
          quiet:
            !(main.has(model.name) || lines.has(model.name)) ||
            QUIET.test(model.name) ||
            !!model.guard?.append_only,
        });
    }
    return out;
  }, [d.apps, d.modules, d.lang]);
}

type Group = { id: string; label: string; items: ModelChoice[] };

export function ModelPicker(p: {
  label: string;
  value: string;
  onChange: (model: string) => void;
  /** Model names used lately, most recent first. */
  recent: string[];
  error?: string;
}) {
  const choices = useModelChoices();
  const { locale } = useI18n();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const id = useId();
  const listId = `${id}-list`;
  const current = choices.find((c) => c.model.name === p.value);

  const groups = useMemo<Group[]>(() => {
    const q = query.trim().toLocaleLowerCase();
    const hit = (c: ModelChoice) =>
      !q ||
      c.title.toLocaleLowerCase().includes(q) ||
      c.appName.toLocaleLowerCase().includes(q) ||
      c.model.name.includes(q.replace(/\s+/g, "_"));
    const shown = choices.filter((c) => hit(c) && (q !== "" || !c.quiet));
    const out: Group[] = [];
    if (!q) {
      const recent = p.recent
        .map((name) => shown.find((c) => c.model.name === name))
        .filter((c): c is ModelChoice => !!c)
        .slice(0, 5);
      if (recent.length) out.push({ id: "recent", label: t("Recently used"), items: recent });
    }
    const byApp = new Map<string, ModelChoice[]>();
    for (const c of shown) byApp.set(c.app.id, [...(byApp.get(c.app.id) ?? []), c]);
    const apps = [...byApp.values()].sort((a, b) => compareText(a[0]!.appName, b[0]!.appName));
    for (const items of apps) out.push({ id: items[0]!.app.id, label: items[0]!.appName, items });
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [choices, query, p.recent.join(), locale]);
  const flat = groups.flatMap((g) => g.items.map((c) => ({ g: g.id, c })));

  useEffect(() => setActive(0), [query, open]);
  useEffect(() => {
    if (!open) return;
    const on = (e: MouseEvent) => {
      if (box.current && !e.composedPath().includes(box.current)) {
        setOpen(false);
        setQuery("");
      }
    };
    addEventListener("mousedown", on);
    return () => removeEventListener("mousedown", on);
  }, [open]);
  useEffect(() => {
    if (!open) return;
    document.getElementById(`${id}-o${active}`)?.scrollIntoView({ block: "nearest" });
  }, [active, open, id]);

  const pick = (name: string) => {
    p.onChange(name);
    setOpen(false);
    setQuery("");
  };
  const keys = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (!open) setOpen(true);
      else setActive((a) => Math.min(a + 1, flat.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter" && open) {
      e.preventDefault();
      e.stopPropagation();
      const o = flat[active];
      if (o) pick(o.c.model.name);
    } else if (e.key === "Escape" && open) {
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
      setQuery("");
    } else if (e.key === "Tab") setOpen(false);
  };

  let n = -1;
  return (
    <div className={`field mpick${p.error ? " has-error" : ""}`} ref={box}>
      <label className="flabel" htmlFor={id}>
        {p.label}
      </label>
      <div className="mpick-box">
        <Icon name="search" size={15} className="mpick-icon" />
        <input
          ref={input}
          id={id}
          role="combobox"
          aria-expanded={open}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={open && flat[active] ? `${id}-o${active}` : undefined}
          aria-invalid={p.error ? true : undefined}
          aria-describedby={p.error ? `${id}-err` : undefined}
          autoComplete="off"
          value={open ? query : current ? `${current.title} · ${current.appName}` : ""}
          placeholder={
            current ? `${current.title} · ${current.appName}` : t("Search apps and records…")
          }
          onFocus={() => setOpen(true)}
          onClick={() => setOpen(true)}
          onChange={(e) => {
            setQuery(e.target.value);
            setOpen(true);
          }}
          onKeyDown={keys}
        />
        <Icon name="chevron-down" size={16} className="mpick-caret" />
      </div>
      {open && (
        <div
          className="mpick-list"
          id={listId}
          role="listbox"
          aria-label={t("{field} options", { field: p.label })}
        >
          {groups.map((g) => (
            <div key={g.id} role="group" aria-labelledby={`${id}-g-${g.id}`}>
              <div className="mpick-group" id={`${id}-g-${g.id}`} role="presentation">
                {g.label}
              </div>
              {g.items.map((c) => {
                n += 1;
                const i = n;
                return (
                  <div
                    key={`${g.id}-${c.model.name}`}
                    id={`${id}-o${i}`}
                    role="option"
                    aria-selected={c.model.name === p.value}
                    className={`mpick-opt${i === active ? " active" : ""}`}
                    onMouseEnter={() => setActive(i)}
                    onMouseDown={(e) => {
                      e.preventDefault();
                      pick(c.model.name);
                    }}
                  >
                    <span className="mpick-name">{c.title}</span>
                    {(g.id === "recent" || query) && <span className="mpick-app">{c.appName}</span>}
                    {c.model.name === p.value && <Icon name="check" size={14} />}
                  </div>
                );
              })}
            </div>
          ))}
          {flat.length === 0 && <div className="mpick-none">{t("No match")}</div>}
          {!query && (
            <div className="mpick-foot">{t("Type to find set-up and log records too.")}</div>
          )}
        </div>
      )}
      {p.error && (
        <p className="error" id={`${id}-err`}>
          <Icon name="warning" size={14} /> {p.error}
        </p>
      )}
    </div>
  );
}
