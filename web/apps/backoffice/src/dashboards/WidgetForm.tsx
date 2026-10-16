import { useMemo, useState } from "react";
import { Button, Dialog, Input, Notice, Select, Textarea } from "@obop/ui";
import type { FieldMeta, ModelMeta } from "../api.ts";
import { useData } from "../data.tsx";
import { label, modelLabel } from "../format.ts";
import { groupable, measurable } from "../query.ts";
import {
  FILTER_HELP,
  OP_NAME,
  formatFilter,
  parseFilter,
  toServerFilter,
  type Condition,
  type WidgetOp,
} from "./filter.ts";
import { KINDS, type Widget, type WidgetKind } from "./widget.ts";

const KIND_NAME: Record<WidgetKind, string> = {
  kpi: "Number (KPI)",
  bar: "Bar chart",
  line: "Line chart",
  pie: "Pie chart",
  table: "Table",
};

const OPS_BY_KIND: Record<FieldMeta["kind"], WidgetOp[]> = {
  text: ["eq", "ne", "contains", "in", "empty", "set"],
  longtext: ["contains", "empty", "set"],
  number: ["eq", "ne", "gt", "gte", "lt", "lte", "empty", "set"],
  money: ["eq", "ne", "gt", "gte", "lt", "lte", "empty", "set"],
  date: ["eq", "ne", "gt", "gte", "lt", "lte", "empty", "set"],
  bool: ["eq"],
  select: ["eq", "ne", "in", "empty", "set"],
  ref: ["eq", "ne", "empty", "set"],
};

type Draft = {
  title: string;
  kind: WidgetKind;
  model_name: string;
  aggregate: "count" | "sum";
  measure_field: string;
  group_by: string;
  conds: Condition[];
  raw: string | null;
};

export type WidgetValues = Omit<Widget, "id" | "dashboard_id" | "position">;

/** Models a dashboard can read: every app's models except the dashboards themselves. */
function useReadable() {
  const d = useData();
  return useMemo(() => {
    const out: { model: ModelMeta; text: string }[] = [];
    for (const a of d.apps) {
      if (a.id === "customize" || a.id === "dashboards") continue;
      for (const m of a.module.models)
        out.push({ model: m, text: `${a.name} › ${modelLabel(m.name, a.module.id)}` });
    }
    return out;
  }, [d.apps]);
}

export function WidgetForm(p: {
  initial?: Widget;
  onSave: (values: WidgetValues) => Promise<string | null>;
  onClose: () => void;
}) {
  const readable = useReadable();
  const w = p.initial;
  const parsed = parseFilter(w?.filter ?? "");
  const [draft, setDraft] = useState<Draft>(() => ({
    title: w?.title ?? "",
    kind: w?.kind ?? "kpi",
    model_name: w?.model_name ?? readable[0]?.model.name ?? "",
    aggregate: w?.aggregate ?? "count",
    measure_field: w?.measure_field ?? "",
    group_by: w?.group_by ?? "",
    conds: "conds" in parsed ? parsed.conds : [],
    raw: "error" in parsed ? (w?.filter ?? "") : null,
  }));
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const set = (patch: Partial<Draft>) => setDraft((x) => ({ ...x, ...patch }));

  const model = readable.find((r) => r.model.name === draft.model_name)?.model;
  // A widget saved for a model this person cannot see can still be edited, but its fields are unknown.
  const fields = model?.fields.filter((f) => f.class !== "secret") ?? [];
  const measures = measurable(fields);
  const dims = groupable(fields);
  const chart = draft.kind === "bar" || draft.kind === "line" || draft.kind === "pie";

  const text = draft.raw ?? formatFilter(draft.conds);

  function changeModel(name: string) {
    set({ model_name: name, measure_field: "", group_by: "", conds: [], raw: null });
  }

  async function save() {
    const errs: Record<string, string> = {};
    if (!draft.title.trim()) errs.title = "Give the tile a title";
    if (!model) errs.model = "Pick a model";
    if (draft.aggregate === "sum" && !draft.measure_field) errs.measure = "Pick a field to add up";
    if (chart && !draft.group_by) errs.group = "A chart needs a field to split by";
    let filterError: string | null = null;
    if (model) {
      const re = parseFilter(text);
      if ("error" in re) filterError = re.error;
      else {
        const sf = toServerFilter(re.conds, model);
        if ("error" in sf) filterError = sf.error;
        else if ("noAccess" in sf) filterError = `${sf.noAccess} is not a field of this model`;
      }
    }
    if (filterError) errs.filter = filterError;
    setErrors(errs);
    if (Object.keys(errs).length) return;
    setBusy(true);
    setError(null);
    const err = await p.onSave({
      title: draft.title.trim(),
      kind: draft.kind,
      model_name: draft.model_name,
      aggregate: draft.aggregate,
      measure_field: draft.aggregate === "sum" ? draft.measure_field : null,
      group_by: draft.group_by || null,
      filter: text.trim() || null,
    });
    setBusy(false);
    if (err) setError(err);
  }

  return (
    <Dialog
      title={w ? "Edit tile" : "Add a tile"}
      size="lg"
      onClose={p.onClose}
      footer={
        <>
          <Button onClick={p.onClose}>Cancel</Button>
          <Button variant="primary" loading={busy} onClick={() => void save()}>
            {w ? "Save tile" : "Add tile"}
          </Button>
        </>
      }
    >
      <form
        className="wform"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <Input
          label="Title"
          value={draft.title}
          error={errors.title}
          required
          onChange={(e) => set({ title: e.target.value })}
        />
        <div className="wform-row">
          <Select
            label="Shows"
            value={draft.kind}
            onChange={(e) => set({ kind: e.target.value as WidgetKind })}
          >
            {KINDS.map((k) => (
              <option key={k} value={k}>
                {KIND_NAME[k]}
              </option>
            ))}
          </Select>
          <Select
            label="Records of"
            value={draft.model_name}
            error={errors.model}
            onChange={(e) => changeModel(e.target.value)}
          >
            {!model && draft.model_name && <option value={draft.model_name}>No access</option>}
            {readable.map((r) => (
              <option key={r.model.name} value={r.model.name}>
                {r.text}
              </option>
            ))}
          </Select>
        </div>
        <div className="wform-row">
          <Select
            label="Combine by"
            value={draft.aggregate}
            onChange={(e) =>
              set({ aggregate: e.target.value as "count" | "sum", measure_field: "" })
            }
          >
            <option value="count">Count of records</option>
            <option value="sum" disabled={measures.length === 0}>
              Sum of a field
            </option>
          </Select>
          {draft.aggregate === "sum" && (
            <Select
              label="Field to add up"
              value={draft.measure_field}
              error={errors.measure}
              onChange={(e) => set({ measure_field: e.target.value })}
            >
              <option value="">Choose a field</option>
              {measures.map((f) => (
                <option key={f.name} value={f.name}>
                  {label(f.name)}
                  {f.kind === "money" ? " (money)" : ""}
                </option>
              ))}
            </Select>
          )}
        </div>
        <Select
          label={chart ? "Split by" : "Split by (optional)"}
          value={draft.group_by}
          error={errors.group}
          hint={
            draft.kind === "kpi"
              ? "A number tile shows one value; leave this empty."
              : draft.kind === "table"
                ? "Empty shows the first records instead of totals."
                : undefined
          }
          onChange={(e) => set({ group_by: e.target.value })}
        >
          <option value="">{chart ? "Choose a field" : "Nothing"}</option>
          {dims.map((f) => (
            <option key={f.name} value={f.name}>
              {label(f.name)}
              {f.kind === "date" ? " (month)" : ""}
            </option>
          ))}
        </Select>

        <fieldset className="wfilter">
          <legend>Only records where</legend>
          {draft.raw !== null ? (
            <Textarea
              label="Filter"
              value={draft.raw}
              error={errors.filter}
              hint={FILTER_HELP}
              onChange={(e) => set({ raw: e.target.value })}
            />
          ) : (
            <>
              {draft.conds.map((c, i) => (
                <ConditionRow
                  key={i}
                  fields={fields}
                  cond={c}
                  onChange={(next) =>
                    set({ conds: draft.conds.map((x, j) => (j === i ? next : x)) })
                  }
                  onRemove={() => set({ conds: draft.conds.filter((_, j) => j !== i) })}
                />
              ))}
              {draft.conds.length === 0 && <p className="hint">All records.</p>}
              <div>
                <Button
                  size="sm"
                  icon="plus"
                  disabled={!model}
                  onClick={() =>
                    set({
                      conds: [
                        ...draft.conds,
                        { field: fields[0]?.name ?? "", op: "eq", value: "" },
                      ],
                    })
                  }
                >
                  Add a condition
                </Button>
              </div>
              {errors.filter && (
                <p className="error" role="alert">
                  {errors.filter}
                </p>
              )}
            </>
          )}
          <p className="hint">
            Saved as <code>{text || "(all records)"}</code>. {draft.raw === null && FILTER_HELP}
          </p>
        </fieldset>
        {error && (
          <Notice tone="danger" title="Could not save the tile">
            {error}
          </Notice>
        )}
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}

function ConditionRow(p: {
  fields: FieldMeta[];
  cond: Condition;
  onChange: (c: Condition) => void;
  onRemove: () => void;
}) {
  const f = p.fields.find((x) => x.name === p.cond.field);
  const ops = f ? OPS_BY_KIND[f.kind] : (["eq"] as WidgetOp[]);
  const needsValue = p.cond.op !== "empty" && p.cond.op !== "set";
  const name = f ? label(f.name) : p.cond.field;
  return (
    <div className="wcond">
      <Select
        label="Field"
        value={p.cond.field}
        onChange={(e) => {
          const nf = p.fields.find((x) => x.name === e.target.value);
          const allowed = nf ? OPS_BY_KIND[nf.kind] : ["eq" as WidgetOp];
          p.onChange({
            field: e.target.value,
            op: allowed.includes(p.cond.op) ? p.cond.op : allowed[0]!,
            value: "",
          });
        }}
      >
        {p.fields.map((x) => (
          <option key={x.name} value={x.name}>
            {label(x.name)}
          </option>
        ))}
      </Select>
      <Select
        label="Condition"
        value={p.cond.op}
        onChange={(e) => p.onChange({ ...p.cond, op: e.target.value as WidgetOp })}
      >
        {ops.map((o) => (
          <option key={o} value={o}>
            {OP_NAME[o]}
          </option>
        ))}
      </Select>
      {needsValue &&
        (f?.kind === "select" && p.cond.op !== "in" ? (
          <Select
            label="Value"
            value={p.cond.value}
            onChange={(e) => p.onChange({ ...p.cond, value: e.target.value })}
          >
            <option value="">Choose</option>
            {f.options?.map((o) => (
              <option key={o} value={o}>
                {label(o)}
              </option>
            ))}
          </Select>
        ) : f?.kind === "bool" ? (
          <Select
            label="Value"
            value={p.cond.value}
            onChange={(e) => p.onChange({ ...p.cond, value: e.target.value })}
          >
            <option value="">Choose</option>
            <option value="true">Yes</option>
            <option value="false">No</option>
          </Select>
        ) : (
          <Input
            label="Value"
            value={p.cond.value}
            placeholder={
              p.cond.op === "in"
                ? "a|b|c"
                : f?.kind === "money"
                  ? "Minor units, 1250 = 12.50"
                  : f?.kind === "date"
                    ? "2026-01-31"
                    : ""
            }
            onChange={(e) => p.onChange({ ...p.cond, value: e.target.value })}
          />
        ))}
      <Button
        variant="ghost"
        size="sm"
        icon="close"
        aria-label={`Remove condition on ${name}`}
        onClick={p.onRemove}
      />
    </div>
  );
}
