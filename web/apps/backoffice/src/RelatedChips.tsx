// The smart buttons of a record (lane L5, plan 2.6 item 7): the records of other models that point
// at it, each with its number (or its sum) from the server. At most four show, the largest first;
// the rest, and every one at zero, wait behind More. A child model drawn as lines in the form gets
// no button.
import { useEffect, useMemo, useState } from "react";
import { fmt, getLang, labelFor, t } from "@obop/ui";
import { aggregate, type FieldMeta, type ModelMeta } from "./api.ts";
import { useData } from "./data.tsx";
import { appTitle, fieldTitle, fieldTitleByName, modelTitle } from "./labels.ts";
import { childrenOf, indexModels } from "./presentation.ts";
import { encodeState } from "./query.ts";

/** How many buttons show before More. */
const SHOWN = 4;

type Rel = { model: string; module: string; field: string; title: string; app: string };

/**
 * A quantity a related record holds (stock on hand): the button shows its sum, not a count. Only a
 * model without a workflow whose package sums a quantity on its cards (`ui.card_sum`) holds stock;
 * order lines and moves are always counted.
 */
function sumField(model: ModelMeta | undefined): FieldMeta | undefined {
  if (!model) return undefined;
  const hasState = model.fields.some(
    (f) => f.kind === "select" && /^(state|stage|status)$/.test(f.name),
  );
  if (hasState) return undefined;
  const sum = model.ui?.card_sum;
  return model.fields.find(
    (f) =>
      f.name === sum &&
      (f.kind === "number" || f.kind === "decimal") &&
      /^(quantity|qty|quantity_on_hand|on_hand)$/.test(f.name),
  );
}

/** The plural a package gives a model (D95 `plural`), else its name. */
function pluralTitle(m: ModelMeta, moduleId: string): string {
  const own = modelTitle(m, moduleId);
  if (!m.plural && !m.plural_labels) return own;
  return labelFor({ labels: m.plural_labels }, getLang(), m.plural ?? own);
}

/** Every ref of another model that points at `model`, named for a button. */
export function useRelated(model: ModelMeta) {
  const d = useData();
  return useMemo(() => {
    const out: Rel[] = [];
    for (const [name, info] of d.models)
      if (name !== model.name)
        for (const f of info.model.fields)
          if (f.kind === "ref" && f.to === model.name)
            out.push({
              model: name,
              module: info.module.id,
              field: f.name,
              title: pluralTitle(info.model, info.module.id),
              app: appTitle(info.module),
            });
    // Two apps can both have an "Order": say which one.
    const models = new Map<string, Set<string>>();
    for (const r of out) models.set(r.title, (models.get(r.title) ?? new Set()).add(r.model));
    return out.map((r) =>
      (models.get(r.title)?.size ?? 0) > 1 ? { ...r, title: `${r.app}: ${r.title}` } : r,
    );
  }, [d.models, model.name, d.lang]);
}

/** The number (or sum) of each relation for record `id`; a relation is absent until known. */
function useCounts(rels: Rel[], id: string): Map<string, number> {
  const d = useData();
  const [counts, setCounts] = useState<{ id: string; n: Map<string, number> }>({
    id,
    n: new Map(),
  });
  const key = rels.map((r) => `${r.model}.${r.field}`).join(",");
  useEffect(() => {
    let live = true;
    setCounts({ id, n: new Map() });
    for (const r of rels) {
      const sum = sumField(d.models.get(r.model)?.model);
      void aggregate(d.token, r.model, {
        filter: [{ field: r.field, op: "eq", value: id }],
        measures: sum ? [{ op: "count" }, { op: "sum", field: sum.name }] : [{ op: "count" }],
      }).then((res) => {
        if (!live) return;
        const n =
          "error" in res
            ? 0
            : sum
              ? Number(res.groups[0]?.measures[`sum:${sum.name}`] ?? 0) || 0
              : res.total;
        setCounts((c) =>
          c.id === id ? { id, n: new Map(c.n).set(`${r.model}.${r.field}`, n) } : c,
        );
      });
    }
    return () => {
      live = false;
    };
    // `rels` is described by `key`.
  }, [d.token, key, id, d.mutations]);
  return counts.id === id ? counts.n : new Map();
}

/** One smart button: a link to the related records, filtered to this record. */
function SmartLink({ rel: r, id, n }: { rel: Rel; id: string; n: number | undefined }) {
  const d = useData();
  const st = encodeState({ facets: [{ op: "is", f: r.field, v: [id] }], group: [] });
  const to = d.linkTo(r.model, { st });
  const sum = sumField(d.models.get(r.model)?.model);
  const body = (
    <>
      <span className="smart-name">
        {sum ? t("{field} in {model}", { field: fieldTitle(sum), model: r.title }) : r.title}
      </span>
      <span className="smart-n">{n === undefined ? "…" : fmt.integer(n)}</span>
    </>
  );
  const title = sum
    ? t("{model}: the sum of {field}", { model: r.title, field: fieldTitleByName(sum.name) })
    : t("{model} linked through {field}", {
        model: r.title,
        field: fieldTitleByName(r.field, d.models.get(r.model)?.model.fields),
      });
  return to ? (
    <a className="smartbtn" href={to} title={title}>
      {body}
    </a>
  ) : (
    <span className="smartbtn disabled" title={title}>
      {body}
    </span>
  );
}

/** The row of smart buttons of a saved record; nothing when no other model points at it. */
export function RelatedChips({ model, id }: { model: ModelMeta; id: string }) {
  const d = useData();
  const all = useRelated(model);
  // Lines drawn inside the form (LinesSection) need no button of their own.
  const related = useMemo(() => {
    const index = indexModels(d.modules);
    const inForm = new Set(
      childrenOf(model, index)
        .filter((c) => c.shape !== "translations")
        .map((c) => `${c.model}.${c.via}`),
    );
    return all.filter((r) => !inForm.has(`${r.model}.${r.field}`));
  }, [all, d.modules, model]);
  const counts = useCounts(related, id);
  const [more, setMore] = useState(false);
  if (related.length === 0) return null;
  const known = related.every((r) => counts.has(`${r.model}.${r.field}`));
  const n = (r: Rel) => counts.get(`${r.model}.${r.field}`);
  // Until every number is in, the order would jump: the buttons wait in declaration order.
  const sorted = known ? [...related].sort((a, b) => (n(b) ?? 0) - (n(a) ?? 0)) : related;
  const top = sorted.filter((r) => (known ? (n(r) ?? 0) > 0 : true)).slice(0, SHOWN);
  const rest = sorted.filter((r) => !top.includes(r));
  return (
    <div className="smart" role="group" aria-label={t("Related records")}>
      {top.map((r) => (
        <SmartLink key={`${r.model}.${r.field}`} rel={r} id={id} n={n(r)} />
      ))}
      {more &&
        rest.map((r) => <SmartLink key={`${r.model}.${r.field}`} rel={r} id={id} n={n(r)} />)}
      {known && rest.length > 0 && (
        <button
          type="button"
          className="smartbtn smart-more"
          aria-expanded={more}
          onClick={() => setMore(!more)}
        >
          {/* Never a number of empty relations: "More (2)" counts relations that hold records. */}
          {more
            ? t("Fewer")
            : rest.some((r) => (n(r) ?? 0) > 0)
              ? t("More ({count})", { count: rest.filter((r) => (n(r) ?? 0) > 0).length })
              : top.length
                ? t("More links")
                : t("Nothing linked yet")}
        </button>
      )}
    </div>
  );
}
