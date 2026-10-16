import { useEffect, useMemo, useRef, useState } from "react";
import type { FieldMeta, ModelMeta, Row } from "./api.ts";
import { useData } from "./data.tsx";
import { fromEdit, toEdit, validate, type Edit } from "./edit.ts";
import { FieldInput, RefLabel } from "./fields.tsx";
import { isMasked, isState, label, modelLabel, optionLabel } from "./format.ts";
import { encodeState } from "./query.ts";
import { nav } from "./route.ts";
import { Button, Icon, IconButton, Kbd } from "./ui.tsx";

type Props = {
  model: ModelMeta;
  fields: FieldMeta[];
  row: Row | null;
  defaults: Record<string, unknown>;
  withheld: string[];
  title: string;
  onClose: () => void;
  onSaved: (id: string) => void;
};

/** The record opens in a floating side panel, so the list stays in view (docs/spec/ux.md 4.1). */
export function RecordPanel({
  model,
  fields,
  row,
  defaults,
  withheld,
  title,
  onClose,
  onSaved,
}: Props) {
  const d = useData();
  const creating = row === null;
  const [draft, setDraft] = useState<Record<string, Edit>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const panel = useRef<HTMLElement>(null);
  const statusField = fields.find(isState);
  const canEdit = creating ? model.can_create : model.can_update;

  const baseline = (f: FieldMeta): Edit => toEdit(f, row ? row[f.name] : defaults[f.name]);
  const value = (f: FieldMeta): Edit => (f.name in draft ? draft[f.name]! : baseline(f));
  const masked = (f: FieldMeta) => !!row && isMasked(row[f.name]) && !(f.name in draft);
  const changed = (f: FieldMeta) => f.name in draft && draft[f.name] !== baseline(f);
  const dirty = fields.some(changed);
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  // Leaving with unsaved edits asks first (hash navigation, tab close).
  useEffect(() => {
    nav.dirty = () => dirtyRef.current;
    const before = (e: BeforeUnloadEvent) => {
      if (dirtyRef.current) e.preventDefault();
    };
    addEventListener("beforeunload", before);
    return () => {
      if (nav.dirty) nav.dirty = null;
      removeEventListener("beforeunload", before);
    };
  }, []);

  // Focus the panel while open, and give focus back where it was.
  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    panel.current?.focus({ preventScroll: true });
    return () => {
      if (opener && document.contains(opener)) opener.focus({ preventScroll: true });
    };
  }, []);

  const saveRef = useRef<() => void>(() => {});
  saveRef.current = () => void save();
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        if (dirtyRef.current || creating) saveRef.current();
      } else if (e.key === "Escape" && !e.defaultPrevented && !document.querySelector(".overlay")) {
        onClose();
      }
    };
    addEventListener("keydown", on);
    return () => removeEventListener("keydown", on);
  }, [onClose, creating]);

  function failure(message: string) {
    const prefix = `${model.name}.`;
    const f = message.startsWith(prefix)
      ? fields.find((x) => message.startsWith(`${prefix}${x.name} `))
      : undefined;
    if (f) setErrors((e) => ({ ...e, [f.name]: message }));
    else setError(message);
  }

  async function save() {
    setError(null);
    const editable = fields.filter((f) => !masked(f));
    const errs = validate(editable, value, creating ? undefined : (f) => changed(f) || f.required);
    setErrors(errs);
    if (Object.keys(errs).length) return d.notify("Fix the highlighted fields first", "error");
    const body: Record<string, unknown> = {};
    for (const f of editable) {
      const r = fromEdit(f, value(f));
      if ("error" in r) continue;
      if (creating) {
        if (r.value !== null && (f.kind !== "bool" || r.value === true || f.name in draft))
          body[f.name] = r.value;
      } else if (changed(f)) body[f.name] = r.value;
    }
    setBusy(true);
    const r = creating
      ? await d.run(`${model.name}.create`, body)
      : await d.run(`${model.name}.update`, { id: row.id, ...body });
    setBusy(false);
    if (r.kind === "ok") {
      d.notify(creating ? `${title} created` : "Saved");
      setDraft({});
      dirtyRef.current = false;
      onSaved(String((r.value as Row).id ?? row?.id));
    } else if (r.kind === "approval") {
      d.notify(
        `Approval needed: the change was proposed (plan ${r.planId.slice(0, 8)}). A human must approve it first.`,
        "info",
      );
    } else failure(r.message);
  }

  async function setStatus(s: string) {
    if (!statusField) return;
    if (creating) return setDraft({ ...draft, [statusField.name]: s });
    if (!model.can_update || busy) return;
    setBusy(true);
    const r = await d.run(`${model.name}.update`, { id: row.id, [statusField.name]: s });
    setBusy(false);
    if (r.kind === "ok") {
      setDraft((x) => {
        const { [statusField.name]: _, ...rest } = x;
        return rest;
      });
      d.notify(`${label(statusField.name)}: ${optionLabel(s)}`);
    } else if (r.kind === "approval")
      d.notify(
        `Approval needed: moving to ${optionLabel(s)} was proposed (plan ${r.planId.slice(0, 8)}).`,
        "info",
      );
    else d.notify(r.message, "error");
  }

  async function act(id: string, needsApproval: boolean) {
    if (!row) return;
    setBusy(true);
    setError(null);
    const r = await d.run(id, { id: row.id });
    setBusy(false);
    const name = label(id.split(".")[1] ?? id);
    if (r.kind === "ok") d.notify(`${name} done`);
    else if (r.kind === "approval")
      d.notify(
        `Approval needed: "${name}" was proposed (plan ${r.planId.slice(0, 8)}). Nothing changes until a human approves it.`,
        "info",
      );
    else {
      setError(r.message);
      d.notify(needsApproval ? `${name} was refused: ${r.message}` : r.message, "error");
    }
  }

  const related = useMemo(() => {
    const out: { model: string; module: string; field: string; title: string }[] = [];
    for (const [name, info] of d.models)
      if (name !== model.name)
        for (const f of info.model.fields)
          if (f.kind === "ref" && f.to === model.name)
            out.push({
              model: name,
              module: info.module.id,
              field: f.name,
              title: modelLabel(name, info.module.id),
            });
    return out;
  }, [d.models, model.name]);

  return (
    <aside className="record panel" aria-label={`${title} record`} ref={panel} tabIndex={-1}>
      <header>
        <div>
          <h2>{creating ? `New ${title.toLowerCase()}` : d.recordLabel(model.name, row)}</h2>
          {!creating && <span className="hint">{title}</span>}
        </div>
        <IconButton icon="close" label="Close record" onClick={onClose} />
      </header>
      {statusField && (
        <ol className="statusbar" aria-label={label(statusField.name)}>
          {statusField.options?.map((s, idx, all) => {
            const on = String(value(statusField)) === s;
            const current = all.indexOf(String(value(statusField)));
            const click = creating || model.can_update;
            return (
              <li key={s}>
                <button
                  className={on ? "on" : idx < current ? "done" : ""}
                  aria-current={on ? "step" : undefined}
                  disabled={!click || busy}
                  onClick={() => void setStatus(s)}
                >
                  {optionLabel(s)}
                </button>
              </li>
            );
          })}
        </ol>
      )}
      {!creating && related.length > 0 && (
        <div className="smart" role="group" aria-label="Related records">
          {related.map((r) => {
            const rows = d.store.peek(r.model);
            const n = rows?.filter((x) => x[r.field] === row.id).length;
            const st = encodeState({
              facets: [{ op: "is", f: r.field, v: [row.id] }],
              group: [],
            });
            const to = d.linkTo(r.model, { st });
            const text = `${r.title} (${n ?? "…"})`;
            return to ? (
              <a
                key={`${r.model}.${r.field}`}
                className="smartbtn"
                href={to}
                title={`${r.title} linked through ${label(r.field)}`}
              >
                {text}
              </a>
            ) : (
              <span key={`${r.model}.${r.field}`} className="smartbtn disabled">
                {text}
              </span>
            );
          })}
        </div>
      )}
      {!creating && model.actions.length > 0 && (
        <div className="actionrow">
          {model.actions.map((a) => (
            <Button
              key={a.id}
              title={`${a.description}${a.needs_approval ? " (needs a human's approval when an agent runs it)" : ""}`}
              disabled={busy}
              onClick={() => void act(a.id, a.needs_approval)}
            >
              {label(a.id.split(".")[1] ?? a.id)}
              {a.needs_approval ? " *" : ""}
            </Button>
          ))}
        </div>
      )}
      <form
        className="fields"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          if (dirty || creating) void save();
        }}
      >
        {fields
          .filter((f) => f !== statusField)
          .map((f) => {
            const isMasked_ = masked(f);
            const fid = `f-${model.name}-${f.name}`;
            const err = errors[f.name];
            return (
              <div key={f.name} className={`field${f.kind === "bool" ? " inline" : ""}`}>
                <label className="flabel" htmlFor={fid}>
                  {label(f.name)}
                  {f.required && (
                    <span className="req" title="Required" aria-hidden="true">
                      *
                    </span>
                  )}
                  {f.personal_data && (
                    <span
                      className="shield"
                      title={`Personal data (${f.class}). Agents see it masked.`}
                      role="img"
                      aria-label={`Personal data, ${f.class}`}
                    >
                      <Icon name="shield" size={13} />
                    </span>
                  )}
                  {f.class === "financial" && <span className="chip">financial</span>}
                </label>
                {isMasked_ ? (
                  <div className="maskedfield">
                    <span className="masked" id={fid}>
                      {String(row?.[f.name])}
                    </span>
                    <span className="hint">Masked for this session</span>
                    {canEdit && (
                      <button
                        type="button"
                        className="linkbtn"
                        onClick={() => setDraft({ ...draft, [f.name]: "" })}
                      >
                        Replace
                      </button>
                    )}
                  </div>
                ) : (
                  <FieldInput
                    field={f}
                    id={fid}
                    value={value(f)}
                    readOnly={!canEdit}
                    invalid={!!err}
                    describedBy={err ? `${fid}-err` : undefined}
                    onChange={(v) => {
                      setDraft({ ...draft, [f.name]: v });
                      if (err) setErrors({ ...errors, [f.name]: "" });
                    }}
                  />
                )}
                {f.kind === "ref" &&
                  !isMasked_ &&
                  String(value(f)) &&
                  d.linkTo(f.to ?? "", { id: String(value(f)) }) && (
                    <a
                      className="reflink small"
                      href={d.linkTo(f.to ?? "", { id: String(value(f)) })!}
                    >
                      Open <RefLabel field={f} id={String(value(f))} link={false} />
                    </a>
                  )}
                {err && (
                  <p id={`${fid}-err`} role="alert" className="error">
                    {err}
                  </p>
                )}
              </div>
            );
          })}
        {withheld.length > 0 && (
          <p className="hint withheld">
            <Icon name="shield" size={13} /> Withheld from you:{" "}
            {withheld.map((w) => label(w.split(".")[1] ?? w)).join(", ")}
          </p>
        )}
        {error && (
          <p role="alert" className="error banner">
            {error}
          </p>
        )}
        <footer className="formbar">
          <Button
            type="submit"
            variant="primary"
            loading={busy}
            disabled={(!dirty && !creating) || !canEdit}
          >
            {creating ? "Create" : "Save"}
          </Button>
          <Button
            onClick={() => {
              if (dirty) {
                setDraft({});
                setErrors({});
                setError(null);
              } else onClose();
            }}
          >
            {dirty ? "Discard" : "Close"}
          </Button>
          {!canEdit && <span className="hint">Read only for you</span>}
          <span className="grow" />
          <span className="hint kbd-hint">
            <Kbd>⌘S</Kbd> save <Kbd>Esc</Kbd> close
          </span>
        </footer>
      </form>
    </aside>
  );
}
