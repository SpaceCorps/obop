import { useMemo, useState } from "react";
import { fmt, getCurrency, msg, t, useI18n } from "@obop/ui";
import { getTime, type FieldMeta, type ModelMeta } from "./api.ts";
import { loadFavorites, saveFavorites, type Favorite } from "./favorites.ts";
import { FieldInput } from "./fields.tsx";
import {
  boolFields,
  dateFilters,
  isOn,
  optionFields,
  optionText,
  quickFilters,
  rangeFields,
  todayIn,
  toggleQuick,
  type QuickFilter,
} from "./filters.ts";
import { optionLabel } from "./format.ts";
import { stateFieldOf } from "./presentation.ts";
import { fieldTitle } from "./labels.ts";
import {
  addFacet,
  boundText,
  groupable,
  toggleOption,
  validRange,
  type Cond,
  type ViewState,
} from "./query.ts";
import { Icon } from "./ui.tsx";

type Props = {
  model: string;
  /** The model's metadata, for the generated filters; without it the panel builds a bare one. */
  meta?: ModelMeta;
  fields: FieldMeta[];
  state: ViewState;
  view: string;
  setState: (s: ViewState) => void;
  applyFavorite: (f: Favorite) => void;
  /** The signed-in user name, for "Mine". */
  user?: string;
  /** The quick filters, when the caller has them already (ControlPanel names chips with them). */
  quick?: QuickFilter[];
};

/** The model the panel works on: the metadata, or a bare one around the fields (the brand book). */
export const metaOr = (meta: ModelMeta | undefined, model: string, fields: FieldMeta[]) =>
  meta ?? {
    name: model,
    description: "",
    can_create: false,
    can_update: false,
    can_delete: false,
    fields,
    actions: [],
  };

/**
 * The quick filters of a model (filters.ts): saved, Open, Overdue, Mine, Archived and the date
 * ranges of its main dates, worked out for today in the instance's zone.
 */
export function useQuickFilters(meta: ModelMeta, fields: FieldMeta[], user?: string) {
  const { lang } = useI18n();
  const today = todayIn(getTime().zone);
  return useMemo(() => {
    const ctx = { user, today, firstDay: fmt.firstDay() };
    return [
      ...quickFilters(meta, fields, ctx),
      ...rangeFields(fields).flatMap((f) => dateFilters(f, ctx)),
    ];
    // The labels are translated: a new language makes them again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meta, fields, user, today, lang]);
}

/** Filters, Group by (with Sort) and Favorites side by side; one column on a phone. */
export function FilterPanel(p: Props) {
  const meta = metaOr(p.meta, p.model, p.fields);
  const own = useQuickFilters(meta, p.fields, p.user);
  const quick = p.quick ?? own;
  return (
    <div className="fpanel" role="group" aria-label={t("Filters, group by and favorites")}>
      <FiltersSection {...p} meta={meta} quick={quick} />
      <GroupSection {...p} />
      <FavoritesSection {...p} />
    </div>
  );
}

function FiltersSection({
  meta,
  fields,
  state,
  setState,
  quick,
}: Props & { meta: ModelMeta; quick: QuickFilter[] }) {
  // The status as checkboxes (it is what a list is most often narrowed by), any other selection
  // as a row of pills.
  const options = optionFields(meta, fields);
  const status = stateFieldOf(meta);
  const state1 = options.find((f) => f.name === status?.name) ?? (status ? undefined : options[0]);
  const others = options.filter((f) => f !== state1);
  const bools = boolFields(fields);
  const plain = quick.filter((q) => q.kind !== "date");
  const dated = rangeFields(fields)
    .map((f) => ({ f, ranges: quick.filter((q) => q.kind === "date" && q.field === f.name) }))
    .filter((x) => x.ranges.length > 0);
  const picked = (f: string, v: string) =>
    state.facets.some((c) => c.op === "is" && c.f === f && c.v.includes(v));
  const toggle = (q: QuickFilter) => setState({ ...state, facets: toggleQuick(state.facets, q) });
  return (
    <section aria-labelledby="fp-filters">
      <h4 id="fp-filters">{t("Filters")}</h4>
      {plain.length > 0 && (
        <div className="fp-group" role="group" aria-label={t("Quick filters")}>
          {plain.map((q) => (
            <label key={q.key} className="fp-item">
              <input type="checkbox" checked={isOn(state.facets, q)} onChange={() => toggle(q)} />
              <span>{q.label}</span>
            </label>
          ))}
        </div>
      )}
      {state1 && (
        <div role="group" aria-label={fieldTitle(state1)} className="fp-group">
          {state1.options?.map((o) => (
            <label key={o} className="fp-item">
              <input
                type="checkbox"
                checked={picked(state1.name, o)}
                onChange={() =>
                  setState({ ...state, facets: toggleOption(state.facets, state1.name, o) })
                }
              />
              <span>{optionText(state1, o)}</span>
            </label>
          ))}
        </div>
      )}
      {others.map((f) => (
        <div key={f.name} className="fp-dates" role="group" aria-label={fieldTitle(f)}>
          <span className="fp-sub">{fieldTitle(f)}</span>
          <div className="fp-pills">
            {f.options?.map((o) => (
              <button
                key={o}
                type="button"
                className="fp-pill"
                aria-pressed={picked(f.name, o)}
                aria-label={optionText(f, o)}
                onClick={() =>
                  setState({ ...state, facets: toggleOption(state.facets, f.name, o) })
                }
              >
                {optionLabel(o, f)}
              </button>
            ))}
          </div>
        </div>
      ))}
      {bools.map((f) => (
        <div key={f.name} className="fp-group">
          {[true, false].map((v) => (
            <label key={String(v)} className="fp-item">
              <input
                type="checkbox"
                checked={state.facets.some((c) => c.op === "bool" && c.f === f.name && c.v === v)}
                onChange={(e) =>
                  setState({
                    ...state,
                    facets: e.target.checked
                      ? addFacet(
                          state.facets.filter((c) => !(c.op === "bool" && c.f === f.name)),
                          { op: "bool", f: f.name, v },
                        )
                      : state.facets.filter(
                          (c) => !(c.op === "bool" && c.f === f.name && c.v === v),
                        ),
                  })
                }
              />
              <span>
                {t("{field}: {values}", { field: fieldTitle(f), values: v ? t("Yes") : t("No") })}
              </span>
            </label>
          ))}
        </div>
      ))}
      {dated.map(({ f, ranges }) => (
        <div key={f.name} className="fp-dates" role="group" aria-label={fieldTitle(f)}>
          <span className="fp-sub">{fieldTitle(f)}</span>
          <div className="fp-pills">
            {ranges.map((q) => (
              <button
                key={q.key}
                type="button"
                className="fp-pill"
                aria-pressed={isOn(state.facets, q)}
                aria-label={q.label}
                onClick={() => toggle(q)}
              >
                {q.range}
              </button>
            ))}
          </div>
        </div>
      ))}
      <CustomFilter
        fields={fields}
        onAdd={(c) => setState({ ...state, facets: addFacet(state.facets, c) })}
      />
    </section>
  );
}

const OPS: Record<string, [string, string][]> = {
  text: [
    ["contains", msg("contains")],
    ["empty", msg("is empty")],
  ],
  select: [
    ["is", msg("is")],
    ["isnot", msg("is not")],
    ["empty", msg("is empty")],
  ],
  ref: [
    ["is", msg("is")],
    ["empty", msg("is empty")],
  ],
  bool: [["bool", msg("is")]],
  range: [
    ["range", msg("between")],
    ["empty", msg("is empty")],
  ],
};
const opsFor = (f: FieldMeta) =>
  OPS[
    f.kind === "number" ||
    f.kind === "money" ||
    f.kind === "date" ||
    f.kind === "datetime" ||
    f.kind === "decimal"
      ? "range"
      : f.kind === "longtext"
        ? "text"
        : f.kind
  ]!;

function CustomFilter({ fields, onAdd }: { fields: FieldMeta[]; onAdd: (c: Cond) => void }) {
  // Images and files have nothing to filter on.
  const usable = fields.filter((x) => x.kind !== "image" && x.kind !== "file");
  const [open, setOpen] = useState(false);
  const [fname, setFname] = useState(usable[0]?.name ?? "");
  const f = usable.find((x) => x.name === fname) ?? usable[0];
  const [op, setOp] = useState("");
  const [v, setV] = useState("");
  const [min, setMin] = useState("");
  const [max, setMax] = useState("");
  if (!f) return null;
  const ops = opsFor(f);
  const cur = ops.some(([o]) => o === op) ? op : ops[0]![0];
  const reset = () => {
    setV("");
    setMin("");
    setMax("");
  };
  const numeric = f.kind === "number" || f.kind === "money" || f.kind === "decimal";
  // A decimal bound may have more digits than the field keeps: the server compares exactly.
  const digits = f.kind === "money" ? fmt.exponent() : Math.max(f.scale ?? 0, 6);
  /** A typed bound as the canonical decimal kept in the address: undefined when blank, null when invalid. */
  const bound = (s: string): string | null | undefined => {
    if (!s.trim()) return undefined;
    if (!numeric) return s;
    const p = fmt.parseNumber(s, digits);
    return p ? `${p.negative ? "-" : ""}${p.int}${p.frac ? `.${p.frac}` : ""}` : null;
  };
  const lo = bound(min);
  const hi = bound(max);
  const ambiguous =
    numeric && [min, max].some((s) => s.trim() && fmt.parseNumber(s, digits)?.ambiguous);
  const build = (): Cond | null => {
    if (cur === "empty") return { op: "empty", f: f.name };
    if (cur === "contains") return v.trim() ? { op: "contains", f: f.name, v: v.trim() } : null;
    if (cur === "is" || cur === "isnot") return v ? { op: cur, f: f.name, v: [v] } : null;
    if (cur === "bool") return { op: "bool", f: f.name, v: v !== "false" };
    if (lo === null || hi === null) return null;
    const c: Cond = { op: "range", f: f.name, min: lo, max: hi };
    return validRange(f, c) ? c : null;
  };
  const cond = build();
  const dated = f.kind === "date" || f.kind === "datetime";
  const rangeType = dated ? "date" : "text";
  return (
    <div className="fp-custom">
      {!open ? (
        <button className="linkbtn" onClick={() => setOpen(true)}>
          <Icon name="plus" size={13} /> {t("Add custom filter")}
        </button>
      ) : (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (cond) {
              onAdd(cond);
              reset();
            }
          }}
        >
          <div className="fp-row">
            <select
              aria-label={t("Filter field")}
              value={f.name}
              onChange={(e) => {
                setFname(e.target.value);
                setOp("");
                reset();
              }}
            >
              {usable.map((x) => (
                <option key={x.name} value={x.name}>
                  {fieldTitle(x)}
                </option>
              ))}
            </select>
            <select
              aria-label={t("Filter operator")}
              value={cur}
              onChange={(e) => setOp(e.target.value)}
            >
              {ops.map(([o, l]) => (
                <option key={o} value={o}>
                  {t(l)}
                </option>
              ))}
            </select>
          </div>
          <div className="fp-row">
            {(cur === "contains" || (cur === "is" && f.kind === "text")) && (
              <input
                aria-label={t("Value")}
                value={v}
                onChange={(e) => setV(e.target.value)}
                placeholder={t("Value")}
              />
            )}
            {(cur === "is" || cur === "isnot") && f.kind === "select" && (
              <select aria-label={t("Value")} value={v} onChange={(e) => setV(e.target.value)}>
                <option value="">{t("Choose…")}</option>
                {f.options?.map((o) => (
                  <option key={o} value={o}>
                    {optionLabel(o, f)}
                  </option>
                ))}
              </select>
            )}
            {cur === "is" && f.kind === "ref" && (
              <FieldInput
                field={{ ...f, required: false }}
                value={v}
                onChange={(x) => setV(String(x))}
              />
            )}
            {cur === "bool" && (
              <select
                aria-label={t("Value")}
                value={v || "true"}
                onChange={(e) => setV(e.target.value)}
              >
                <option value="true">{t("Yes")}</option>
                <option value="false">{t("No")}</option>
              </select>
            )}
            {cur === "range" && (
              <>
                <input
                  aria-label={t("From")}
                  type={rangeType}
                  inputMode={dated ? undefined : "decimal"}
                  placeholder={t("From")}
                  value={min}
                  onChange={(e) => setMin(e.target.value)}
                />
                <input
                  aria-label={t("To")}
                  type={rangeType}
                  inputMode={dated ? undefined : "decimal"}
                  placeholder={t("To")}
                  value={max}
                  onChange={(e) => setMax(e.target.value)}
                />
              </>
            )}
            <button type="submit" className="btn small primary" disabled={!cond}>
              {t("Add")}
            </button>
          </div>
          {f.kind === "money" && cur === "range" && (
            <p className="hint">{t("Amounts in {currency}.", { currency: getCurrency() })}</p>
          )}
          {f.kind === "datetime" && cur === "range" && (
            <p className="hint">{t("Days in {zone}.", { zone: getTime().zone })}</p>
          )}
          {ambiguous && cur === "range" && (
            <p className="hint" role="status">
              {t("Read as {from} to {to}", {
                from: lo === undefined || lo === null ? "…" : boundText(f, lo),
                to: hi === undefined || hi === null ? "…" : boundText(f, hi),
              })}
            </p>
          )}
        </form>
      )}
    </div>
  );
}

/** The kinds a sort can follow (the list's column heads sort the same ones). */
const SORTABLE = new Set([
  "text",
  "number",
  "money",
  "decimal",
  "date",
  "datetime",
  "select",
  "bool",
  "ref",
]);

function GroupSection({ fields, state, view, setState }: Props) {
  const gf = groupable(fields);
  // A board has one grouping: its columns. A pick replaces it.
  const single = view === "kanban";
  const toggle = (name: string) =>
    setState({
      ...state,
      group: single
        ? [name]
        : state.group.includes(name)
          ? state.group.filter((g) => g !== name)
          : [...state.group, name],
    });
  const sortable = fields.filter((f) => SORTABLE.has(f.kind) && f.class !== "secret");
  const sort = state.sort;
  return (
    <section aria-labelledby="fp-group">
      <h4 id="fp-group">{t("Group by")}</h4>
      {gf.length === 0 && <p className="hint">{t("Nothing to group by in this model.")}</p>}
      {gf.map((f) => {
        const i = state.group.indexOf(f.name);
        return (
          <label key={f.name} className="fp-item">
            <input
              type={single ? "radio" : "checkbox"}
              name={single ? "fp-group-by" : undefined}
              checked={single ? (state.group[0] ?? "") === f.name : i >= 0}
              onChange={() => toggle(f.name)}
            />
            <span>
              {f.kind === "date" ? t("{field}: Month", { field: fieldTitle(f) }) : fieldTitle(f)}
            </span>
            {!single && i >= 0 && state.group.length > 1 && <span className="chip">{i + 1}</span>}
          </label>
        );
      })}
      {view !== "pivot" && view !== "graph" && sortable.length > 0 && (
        <div className="fp-sort">
          <h4 id="fp-sort">{t("Sort by")}</h4>
          <div className="fp-row">
            <select
              aria-labelledby="fp-sort"
              value={sort?.f ?? ""}
              onChange={(e) =>
                setState({
                  ...state,
                  sort: e.target.value ? { f: e.target.value, dir: sort?.dir ?? "asc" } : undefined,
                })
              }
            >
              <option value="">{t("Default order")}</option>
              {sortable.map((f) => (
                <option key={f.name} value={f.name}>
                  {fieldTitle(f)}
                </option>
              ))}
            </select>
            {sort && (
              <button
                type="button"
                className="btn small"
                aria-label={sort.dir === "asc" ? t("Ascending") : t("Descending")}
                onClick={() =>
                  setState({
                    ...state,
                    sort: { f: sort.f, dir: sort.dir === "asc" ? "desc" : "asc" },
                  })
                }
              >
                <Icon name={sort.dir === "asc" ? "arrow-up" : "arrow-down"} size={14} />
                {sort.dir === "asc" ? t("Ascending") : t("Descending")}
              </button>
            )}
          </div>
        </div>
      )}
    </section>
  );
}

function FavoritesSection({ model, state, view, setState, applyFavorite }: Props) {
  const [favs, setFavs] = useState(() => loadFavorites(model));
  const [name, setName] = useState("");
  const [isDefault, setIsDefault] = useState(false);
  const persist = (next: Favorite[]) => {
    setFavs(next);
    saveFavorites(model, next);
  };
  const save = () => {
    const n = name.trim();
    if (!n) return;
    const fav: Favorite = { id: crypto.randomUUID(), name: n, view, state, isDefault };
    persist([...favs.map((f) => (isDefault ? { ...f, isDefault: false } : f)), fav]);
    setName("");
    setIsDefault(false);
  };
  return (
    <section aria-labelledby="fp-fav">
      <h4 id="fp-fav">{t("Favorites")}</h4>
      {favs.length === 0 && <p className="hint">{t("Save a search to come back to it.")}</p>}
      <ul className="fp-favs">
        {favs.map((f) => (
          <li key={f.id}>
            <button className="fp-fav-apply" onClick={() => applyFavorite(f)}>
              {f.name}
            </button>
            <button
              className={`icon-btn${f.isDefault ? " on" : ""}`}
              aria-pressed={!!f.isDefault}
              aria-label={
                f.isDefault
                  ? t("Remove {name} as default", { name: f.name })
                  : t("Make {name} the default", { name: f.name })
              }
              title={t("Default")}
              onClick={() =>
                persist(favs.map((x) => ({ ...x, isDefault: x.id === f.id && !f.isDefault })))
              }
            >
              <Icon name="star" size={14} />
            </button>
            <button
              className="icon-btn"
              aria-label={t("Delete {name}", { name: f.name })}
              onClick={() => persist(favs.filter((x) => x.id !== f.id))}
            >
              <Icon name="close" size={13} />
            </button>
          </li>
        ))}
      </ul>
      <form
        className="fp-save"
        onSubmit={(e) => {
          e.preventDefault();
          save();
        }}
      >
        <input
          aria-label={t("Name for this search")}
          placeholder={t("Name this search")}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <label className="check">
          <input
            type="checkbox"
            checked={isDefault}
            onChange={(e) => setIsDefault(e.target.checked)}
          />
          {t("Use by default")}
        </label>
        <button type="submit" className="btn small" disabled={!name.trim()}>
          {t("Save current search")}
        </button>
      </form>
      {(state.facets.length > 0 || state.group.length > 0) && (
        <button className="linkbtn" onClick={() => setState({ facets: [], group: [] })}>
          {t("Clear all")}
        </button>
      )}
    </section>
  );
}
