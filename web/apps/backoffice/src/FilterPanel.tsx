import { useState } from "react";
import { fmt, getCurrency, msg, t } from "@obop/ui";
import type { FieldMeta } from "./api.ts";
import { loadFavorites, saveFavorites, type Favorite } from "./favorites.ts";
import { FieldInput } from "./fields.tsx";
import { optionLabel } from "./format.ts";
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
  fields: FieldMeta[];
  state: ViewState;
  view: string;
  setState: (s: ViewState) => void;
  applyFavorite: (f: Favorite) => void;
};

/** The Odoo-style dropdown: Filters, Group By and Favorites side by side. */
export function FilterPanel(p: Props) {
  return (
    <div className="fpanel" role="group" aria-label={t("Filters, group by and favorites")}>
      <FiltersSection {...p} />
      <GroupSection {...p} />
      <FavoritesSection {...p} />
    </div>
  );
}

function FiltersSection({ fields, state, setState }: Props) {
  const quick = fields.filter((f) => f.kind === "select").slice(0, 2);
  const bools = fields.filter((f) => f.kind === "bool").slice(0, 2);
  const picked = (f: string, v: string) =>
    state.facets.some((c) => c.op === "is" && c.f === f && c.v.includes(v));
  return (
    <section aria-labelledby="fp-filters">
      <h4 id="fp-filters">{t("Filters")}</h4>
      {quick.map((f) => (
        <div key={f.name} role="group" aria-label={fieldTitle(f)} className="fp-group">
          {f.options?.map((o) => (
            <label key={o} className="fp-item">
              <input
                type="checkbox"
                checked={picked(f.name, o)}
                onChange={() =>
                  setState({ ...state, facets: toggleOption(state.facets, f.name, o) })
                }
              />
              <span>
                {t("{field}: {values}", { field: fieldTitle(f), values: optionLabel(o, f) })}
              </span>
            </label>
          ))}
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
    f.kind === "number" || f.kind === "money" || f.kind === "date"
      ? "range"
      : f.kind === "longtext"
        ? "text"
        : f.kind
  ]!;

function CustomFilter({ fields, onAdd }: { fields: FieldMeta[]; onAdd: (c: Cond) => void }) {
  const [open, setOpen] = useState(false);
  const [fname, setFname] = useState(fields[0]?.name ?? "");
  const f = fields.find((x) => x.name === fname) ?? fields[0];
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
  const numeric = f.kind === "number" || f.kind === "money";
  /** A typed bound as the canonical decimal kept in the address: undefined when blank, null when invalid. */
  const bound = (s: string): string | null | undefined => {
    if (!s.trim()) return undefined;
    if (!numeric) return s;
    const p = fmt.parseNumber(s, f.kind === "money" ? fmt.exponent() : 6);
    return p ? `${p.negative ? "-" : ""}${p.int}${p.frac ? `.${p.frac}` : ""}` : null;
  };
  const lo = bound(min);
  const hi = bound(max);
  const ambiguous =
    numeric &&
    [min, max].some(
      (s) => s.trim() && fmt.parseNumber(s, f.kind === "money" ? fmt.exponent() : 6)?.ambiguous,
    );
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
  const rangeType = f.kind === "date" ? "date" : "text";
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
              {fields.map((x) => (
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
                  inputMode={f.kind === "date" ? undefined : "decimal"}
                  placeholder={t("From")}
                  value={min}
                  onChange={(e) => setMin(e.target.value)}
                />
                <input
                  aria-label={t("To")}
                  type={rangeType}
                  inputMode={f.kind === "date" ? undefined : "decimal"}
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

function GroupSection({ fields, state, setState }: Props) {
  const gf = groupable(fields);
  const toggle = (name: string) =>
    setState({
      ...state,
      group: state.group.includes(name)
        ? state.group.filter((g) => g !== name)
        : [...state.group, name],
    });
  return (
    <section aria-labelledby="fp-group">
      <h4 id="fp-group">{t("Group by")}</h4>
      {gf.length === 0 && <p className="hint">{t("Nothing to group by in this model.")}</p>}
      {gf.map((f) => {
        const i = state.group.indexOf(f.name);
        return (
          <label key={f.name} className="fp-item">
            <input type="checkbox" checked={i >= 0} onChange={() => toggle(f.name)} />
            <span>
              {f.kind === "date" ? t("{field}: Month", { field: fieldTitle(f) }) : fieldTitle(f)}
            </span>
            {i >= 0 && state.group.length > 1 && <span className="chip">{i + 1}</span>}
          </label>
        );
      })}
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
