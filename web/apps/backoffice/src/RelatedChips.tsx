// The smart buttons of a record: the records of other models that point at it, each with its
// number from the server. Split from RecordPanel.tsx (F2); lane L5 owns it.
import { useEffect, useMemo, useState } from "react";
import { t } from "@obop/ui";
import { aggregate, type ModelMeta } from "./api.ts";
import { useData } from "./data.tsx";
import { appTitle, fieldTitleByName, modelTitle } from "./labels.ts";
import { encodeState } from "./query.ts";

/**
 * A smart button: the related records of one model, with their number from the server (D4), so
 * a related model with thousands of records is never loaded just to be counted.
 */
function SmartLink({
  rel: r,
  id,
}: {
  rel: { model: string; field: string; title: string };
  id: string;
}) {
  const d = useData();
  const n = useCount(d.token, r.model, r.field, id, d.mutations);
  const st = encodeState({ facets: [{ op: "is", f: r.field, v: [id] }], group: [] });
  const to = d.linkTo(r.model, { st });
  const text = `${r.title} (${n === null ? "…" : n})`;
  return to ? (
    <a
      className="smartbtn"
      href={to}
      title={t("{model} linked through {field}", {
        model: r.title,
        field: fieldTitleByName(r.field, d.models.get(r.model)?.model.fields),
      })}
    >
      {text}
    </a>
  ) : (
    <span className="smartbtn disabled">{text}</span>
  );
}

/** How many records of `model` point at `id` through `field`; `null` until known. */
function useCount(
  token: string,
  model: string,
  field: string,
  id: string,
  mutations: number,
): number | null {
  const [n, setN] = useState<{ key: string; n: number } | null>(null);
  const key = `${model}.${field}=${id}`;
  useEffect(() => {
    let live = true;
    void aggregate(token, model, {
      filter: [{ field, op: "eq", value: id }],
      measures: [{ op: "count" }],
    }).then((r) => {
      if (live) setN({ key, n: "error" in r ? 0 : r.total });
    });
    return () => {
      live = false;
    };
  }, [token, model, field, id, key, mutations]);
  return n && n.key === key ? n.n : null;
}

/** Every ref of another model that points at `model`, named for a button. */
export function useRelated(model: ModelMeta) {
  const d = useData();
  return useMemo(() => {
    const out: { model: string; module: string; field: string; title: string; app: string }[] = [];
    for (const [name, info] of d.models)
      if (name !== model.name)
        for (const f of info.model.fields)
          if (f.kind === "ref" && f.to === model.name)
            out.push({
              model: name,
              module: info.module.id,
              field: f.name,
              title: modelTitle(info.model, info.module.id),
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

/** The row of smart buttons of a saved record; nothing when no other model points at it. */
export function RelatedChips({ model, id }: { model: ModelMeta; id: string }) {
  const related = useRelated(model);
  if (related.length === 0) return null;
  return (
    <div className="smart" role="group" aria-label={t("Related records")}>
      {related.map((r) => (
        <SmartLink key={`${r.model}.${r.field}`} rel={r} id={id} />
      ))}
    </div>
  );
}
