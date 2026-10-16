import { useMemo, useState } from "react";
import { call, type FieldMeta, type ModelMeta, type Row } from "./api.ts";
import { isMasked, isMoney, isState, label } from "./format.ts";

type Props = {
  token: string;
  model: ModelMeta;
  fields: FieldMeta[];
  row: Row | null;
  allRows: Row[];
  title: string;
  onClose: () => void;
  onSaved: (id: string) => Promise<void>;
  notify: (t: string) => void;
};

/** The record opens in a floating side panel, so the list stays in view (docs/spec/ux.md 4.1). */
export function RecordPanel({
  token,
  model,
  fields,
  row,
  allRows,
  title,
  onClose,
  onSaved,
  notify,
}: Props) {
  const creating = row === null;
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const stateField = fields.find((f) => isState(f.name));
  const states = useMemo(() => {
    if (!stateField) return [];
    const seen: string[] = [];
    for (const r of allRows) {
      const v = String(r[stateField.name] ?? "");
      if (v && !seen.includes(v)) seen.push(v);
    }
    return seen;
  }, [allRows, stateField]);

  const current = (f: FieldMeta): string => {
    if (f.name in draft) return draft[f.name]!;
    const v = row?.[f.name];
    if (v === null || v === undefined) return "";
    return isMoney(f.name) ? String(Number(v) / 100) : String(v);
  };
  const dirty = Object.keys(draft).length > 0;
  const canEdit = creating ? model.can_create : model.can_update;

  async function save() {
    setBusy(true);
    setError(null);
    const body: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(draft))
      body[k] = isMoney(k) ? Math.round(Number(v) * 100) : v;
    const r = creating
      ? await call(token, `${model.name}.create`, body)
      : await call(token, `${model.name}.update`, { id: row!.id, ...body });
    setBusy(false);
    if (r.kind === "ok") {
      notify(creating ? "Created" : "Saved");
      setDraft({});
      await onSaved(String((r.value as Row).id));
    } else setError(r.kind === "error" ? r.message : "Needs approval");
  }

  async function act(id: string) {
    setBusy(true);
    const r = await call(token, id, { id: row!.id });
    setBusy(false);
    if (r.kind === "ok") {
      notify("Done");
      await onSaved(row!.id);
    } else if (r.kind === "approval") notify("Proposed. A human has to approve it.");
    else setError(r.kind === "error" ? r.message : "");
  }

  return (
    <aside className="record panel" aria-label={`${title} record`}>
      <header>
        <h2>{creating ? `New ${title.toLowerCase()}` : title}</h2>
        <button className="icon-btn" onClick={onClose} aria-label="Close">
          ×
        </button>
      </header>
      {!creating && stateField && states.length > 1 && (
        <ol className="statusbar" aria-label="Status">
          {states.map((s) => (
            <li key={s} aria-current={String(row?.[stateField.name]) === s}>
              {s.replace(/_/g, " ")}
            </li>
          ))}
        </ol>
      )}
      {!creating && model.actions.length > 0 && (
        <div className="actionrow">
          {model.actions.map((a) => (
            <button
              key={a.id}
              className="btn"
              title={a.description}
              disabled={busy}
              onClick={() => act(a.id)}
            >
              {label(a.id.split(".")[1] ?? a.id)}
              {a.needs_approval ? " *" : ""}
            </button>
          ))}
        </div>
      )}
      <form
        className="fields"
        onSubmit={(e) => {
          e.preventDefault();
          if (dirty) void save();
        }}
      >
        {fields.map((f) => {
          const v = current(f);
          const masked = isMasked(row?.[f.name]) && !(f.name in draft);
          return (
            <label key={f.name} className="field">
              <span className="flabel">
                {label(f.name)}
                {f.personal_data && (
                  <span
                    className="shield"
                    title={`Personal data (${f.class}). Agents see it masked.`}
                  >
                    🛡
                  </span>
                )}
                {f.class === "financial" && <span className="chip">financial</span>}
              </span>
              <input
                value={v}
                type={isMoney(f.name) ? "number" : "text"}
                step={isMoney(f.name) ? "0.01" : undefined}
                readOnly={!canEdit || masked}
                list={isState(f.name) ? `states-${model.name}` : undefined}
                onChange={(e) => setDraft({ ...draft, [f.name]: e.target.value })}
                placeholder={f.description}
              />
              {isState(f.name) && (
                <datalist id={`states-${model.name}`}>
                  {states.map((s) => (
                    <option key={s} value={s} />
                  ))}
                </datalist>
              )}
              {masked && <span className="hint">Masked for this session</span>}
            </label>
          );
        })}
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <footer className="formbar">
          <button type="submit" className="btn primary" disabled={!dirty || busy || !canEdit}>
            {creating ? "Create" : "Save"}
          </button>
          <button type="button" className="btn" onClick={() => (dirty ? setDraft({}) : onClose())}>
            {dirty ? "Discard" : "Close"}
          </button>
        </footer>
      </form>
    </aside>
  );
}
