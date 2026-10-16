import { useEffect, useMemo, useRef, useState } from "react";
import type { FieldMeta, ModelMeta, Row } from "./api.ts";
import { useData } from "./data.tsx";
import { fromEdit, toEdit, validate, type Edit } from "./edit.ts";
import { FieldInput, RefLabel } from "./fields.tsx";
import { isMasked, isState, label, modelLabel, optionLabel } from "./format.ts";
import { DeleteDialog } from "./DeleteDialog.tsx";
import { encodeState } from "./query.ts";
import { nav } from "./route.ts";
import { Badge, Button, Icon, IconButton, Kbd, Menu, Tooltip } from "./ui.tsx";
import { actionLabel, canMove, initialOption, isGoverned, moveRecord } from "./workflow.ts";

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
  const [deleting, setDeleting] = useState(false);
  const panel = useRef<HTMLElement>(null);
  const statusField = fields.find(isState);
  const canEdit = creating ? model.can_create : model.can_update;

  // A governed field (D48) starts at its first option and then moves only through actions.
  const governed = (f: FieldMeta) => isGoverned(model, f);
  const baseline = (f: FieldMeta): Edit =>
    creating && governed(f) ? initialOption(f) : toEdit(f, row ? row[f.name] : defaults[f.name]);
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
        if (governed(f)) continue; // the server starts it at the first option
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
    if (!statusField || creating || busy) return;
    if (!governed(statusField) && !model.can_update) return;
    setBusy(true);
    setError(null);
    const out = await moveRecord(d.run, model, statusField, row, s, title);
    setBusy(false);
    if (out.kind === "ok") {
      setDraft((x) => {
        const { [statusField.name]: _, ...rest } = x;
        return rest;
      });
      d.notify(`${label(statusField.name)}: ${optionLabel(s)}`);
    } else if (out.kind === "approval") d.notify(out.message, "info");
    else {
      // The server's own words (D48), also inline so they stay readable.
      setError(out.message);
      d.notify(out.message, "error");
    }
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
            const gov = governed(statusField);
            const cls = on ? "on" : idx < current ? "done" : "";
            if (!gov) {
              const click = creating || model.can_update;
              return (
                <li key={s}>
                  <button
                    className={cls}
                    aria-current={on ? "step" : undefined}
                    disabled={!click || busy}
                    onClick={() =>
                      creating ? setDraft({ ...draft, [statusField.name]: s }) : void setStatus(s)
                    }
                  >
                    {optionLabel(s)}
                  </button>
                </li>
              );
            }
            // Governed: a step is a move, enabled only when an action the person may run leads there.
            const mv = creating
              ? {
                  allowed: false,
                  current: on,
                  needsApproval: false,
                  reason: `A new ${title.toLowerCase()} starts as ${optionLabel(initialOption(statusField))}; it moves on through actions`,
                }
              : canMove(model, statusField, row[statusField.name] as string, s, title);
            const blocked = !mv.allowed;
            const text = `${optionLabel(s)}${mv.allowed && mv.needsApproval ? " *" : ""}`;
            const btn = (
              <button
                className={`${cls}${blocked && !on ? " blocked" : ""}`}
                aria-current={on ? "step" : undefined}
                aria-disabled={(blocked && !on) || busy || undefined}
                data-needs-approval={mv.needsApproval || undefined}
                title={
                  mv.allowed && mv.needsApproval
                    ? "Needs a human's approval when an agent does it"
                    : undefined
                }
                onClick={() => {
                  if (mv.allowed && !busy) void setStatus(s);
                }}
              >
                {text}
              </button>
            );
            return (
              <li key={s}>
                {mv.reason && !on ? (
                  <Tooltip
                    label={mv.reason}
                    side="top"
                    align={idx < all.length / 2 ? "start" : "end"}
                  >
                    {btn}
                  </Tooltip>
                ) : (
                  btn
                )}
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
      {!creating && (model.actions.length > 0 || model.can_delete) && (
        <div className="actionrow">
          {model.actions.map((a, ai) => {
            const f = a.field ? fields.find((x) => x.name === a.field) : undefined;
            const at = f ? String(row[f.name] ?? "") : "";
            const why =
              f && a.to && at === a.to
                ? `Already ${optionLabel(a.to)}`
                : f && a.from && at !== a.from
                  ? `Only available when ${label(f.name)} is ${optionLabel(a.from)}`
                  : "";
            const btn = (
              <Button
                key={a.id}
                title={
                  why
                    ? undefined
                    : `${a.description}${a.needs_approval ? " (needs a human's approval when an agent runs it)" : ""}`
                }
                aria-disabled={why || busy ? true : undefined}
                onClick={() => {
                  if (!why && !busy) void act(a.id, a.needs_approval);
                }}
              >
                {actionLabel(a)}
                {a.needs_approval ? " *" : ""}
              </Button>
            );
            return why ? (
              <Tooltip
                key={a.id}
                label={why}
                side="top"
                align={ai < model.actions.length / 2 ? "start" : "end"}
              >
                {btn}
              </Tooltip>
            ) : (
              btn
            );
          })}
          {model.can_delete && (
            <Button
              variant="ghost"
              className="danger-text"
              icon="trash"
              disabled={busy}
              onClick={() => setDeleting(true)}
            >
              Delete
            </Button>
          )}
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
                ) : governed(f) ? (
                  <GovernedValue
                    model={model}
                    field={f}
                    row={row}
                    title={title}
                    id={fid}
                    busy={busy}
                    onMove={async (to) => {
                      if (!row) return;
                      setBusy(true);
                      setError(null);
                      const out = await moveRecord(d.run, model, f, row, to, title);
                      setBusy(false);
                      if (out.kind === "ok") d.notify(`${label(f.name)}: ${optionLabel(to)}`);
                      else if (out.kind === "approval") d.notify(out.message, "info");
                      else {
                        setError(out.message);
                        d.notify(out.message, "error");
                      }
                    }}
                  />
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
      {deleting && row && (
        <DeleteDialog
          model={model}
          noun={title.toLowerCase()}
          records={[{ id: row.id, label: d.recordLabel(model.name, row) }]}
          onClose={() => setDeleting(false)}
          onDone={() => onClose()}
        />
      )}
    </aside>
  );
}

/** A governed field on the form: its value as a badge, and the moves the person may make. */
function GovernedValue(p: {
  model: ModelMeta;
  field: FieldMeta;
  row: Row | null;
  title: string;
  id: string;
  busy: boolean;
  onMove: (to: string) => void;
}) {
  const { model, field: f, row } = p;
  const current = row ? String(row[f.name] ?? "") : initialOption(f);
  return (
    <div className="governed" id={p.id}>
      {current ? <Badge field={f} value={current} /> : <span className="dim">—</span>}
      {row ? (
        <Menu
          ariaLabel={`Move ${label(f.name).toLowerCase()}`}
          triggerClassName="btn small"
          trigger={<>Move to…</>}
          items={(f.options ?? [])
            .filter((o) => o !== current)
            .map((o) => {
              const mv = canMove(model, f, current, o, p.title);
              return {
                label: `${optionLabel(o)}${mv.needsApproval ? " *" : ""}`,
                disabled: p.busy || !mv.allowed,
                description: mv.reason || undefined,
                onSelect: () => p.onMove(o),
              };
            })}
        />
      ) : (
        <span className="hint">
          Starts as {optionLabel(initialOption(f))}; it moves on through actions.
        </span>
      )}
    </div>
  );
}
