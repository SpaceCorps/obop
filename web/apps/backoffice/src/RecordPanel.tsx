import { useEffect, useMemo, useRef, useState } from "react";
import { describeFor, inSentence, rich, t } from "@obop/ui";
import type { FieldMeta, ModelMeta, Row } from "./api.ts";
import { useData } from "./data.tsx";
import { fromEdit, toEdit, validate, type Edit } from "./edit.ts";
import { FieldInput, RefLabel } from "./fields.tsx";
import { isMasked, isState, optionLabel, rowCurrency } from "./format.ts";
import { actionTitle, fieldTitle, fieldTitleByName, modelTitle } from "./labels.ts";
import { serverMessage } from "./serverMessage.ts";
import { DeleteDialog } from "./DeleteDialog.tsx";
import { encodeState } from "./query.ts";
import { nav } from "./route.ts";
import { Badge, Button, Icon, IconButton, Kbd, Menu, Tooltip } from "./ui.tsx";
import { canMove, initialOption, isGoverned, moveRecord } from "./workflow.ts";

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
  const currency = rowCurrency(row ?? (defaults as Record<string, unknown>));
  const baseline = (f: FieldMeta): Edit =>
    creating && governed(f)
      ? initialOption(f)
      : toEdit(f, row ? row[f.name] : defaults[f.name], currency);
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
    // The server's English is matched first, then shown in the person's language.
    if (f) setErrors((e) => ({ ...e, [f.name]: serverMessage(message) }));
    else setError(serverMessage(message));
  }

  async function save() {
    setError(null);
    const editable = fields.filter((f) => !masked(f));
    const errs = validate(
      editable,
      value,
      creating ? undefined : (f) => changed(f) || f.required,
      currency,
    );
    setErrors(errs);
    if (Object.keys(errs).length) return d.notify(t("Fix the highlighted fields first"), "error");
    const body: Record<string, unknown> = {};
    for (const f of editable) {
      const r = fromEdit(f, value(f), currency);
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
      d.notify(creating ? t("{model} created", { model: title }) : t("Saved"));
      setDraft({});
      dirtyRef.current = false;
      onSaved(String((r.value as Row).id ?? row?.id));
    } else if (r.kind === "approval") {
      d.notify(
        t(
          "Approval needed: the change was proposed (plan {plan}). A human must approve it first.",
          {
            plan: r.planId.slice(0, 8),
          },
        ),
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
      d.notify(
        t("{field}: {values}", {
          field: fieldTitle(statusField),
          values: optionLabel(s, statusField),
        }),
      );
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
    const act = model.actions.find((a) => a.id === id);
    const name = act ? actionTitle(act) : actionTitle({ id });
    if (r.kind === "ok") d.notify(t("{name} done", { name }));
    else if (r.kind === "approval")
      d.notify(
        t(
          "Approval needed: “{name}” was proposed (plan {plan}). Nothing changes until a human approves it.",
          { name, plan: r.planId.slice(0, 8) },
        ),
        "info",
      );
    else {
      const message = serverMessage(r.message);
      setError(message);
      d.notify(
        needsApproval ? t("{name} was refused: {reason}", { name, reason: message }) : message,
        "error",
      );
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
              title: modelTitle(info.model, info.module.id),
            });
    return out;
  }, [d.models, model.name]);

  return (
    <aside
      className="record panel"
      aria-label={t("{model} record", { model: title })}
      ref={panel}
      tabIndex={-1}
    >
      <header>
        <div>
          <h2>
            {creating
              ? t("New {model}", { model: inSentence(title) })
              : d.recordLabel(model.name, row)}
          </h2>
          {!creating && <span className="hint">{title}</span>}
        </div>
        <IconButton icon="close" label={t("Close record")} onClick={onClose} />
      </header>
      {statusField && (
        <ol className="statusbar" aria-label={fieldTitle(statusField)}>
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
                    {optionLabel(s, statusField)}
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
                  reason: t("A new {noun} starts as {state}; it moves on through actions", {
                    noun: inSentence(title),
                    state: optionLabel(initialOption(statusField), statusField),
                  }),
                }
              : canMove(model, statusField, row[statusField.name] as string, s, title);
            const blocked = !mv.allowed;
            const text = `${optionLabel(s, statusField)}${mv.allowed && mv.needsApproval ? " *" : ""}`;
            const btn = (
              <button
                className={`${cls}${blocked && !on ? " blocked" : ""}`}
                aria-current={on ? "step" : undefined}
                aria-disabled={(blocked && !on) || busy || undefined}
                data-needs-approval={mv.needsApproval || undefined}
                title={
                  mv.allowed && mv.needsApproval
                    ? t("Needs a human's approval when an agent does it")
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
                    align={idx === 0 ? "start" : idx === all.length - 1 ? "end" : "center"}
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
        <div className="smart" role="group" aria-label={t("Related records")}>
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
                title={t("{model} linked through {field}", {
                  model: r.title,
                  field: fieldTitleByName(r.field, d.models.get(r.model)?.model.fields),
                })}
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
                ? t("Already {state}", { state: optionLabel(a.to, f) })
                : f && a.from && at !== a.from
                  ? t("Only available when {field} is {state}", {
                      field: fieldTitle(f),
                      state: optionLabel(a.from, f),
                    })
                  : "";
            const btn = (
              <Button
                key={a.id}
                title={
                  why
                    ? undefined
                    : `${describeFor(a)}${a.needs_approval ? ` ${t("(needs a human's approval when an agent runs it)")}` : ""}`
                }
                aria-disabled={why || busy ? true : undefined}
                onClick={() => {
                  if (!why && !busy) void act(a.id, a.needs_approval);
                }}
              >
                {actionTitle(a)}
                {a.needs_approval ? " *" : ""}
              </Button>
            );
            return why ? (
              <Tooltip
                key={a.id}
                label={why}
                side="top"
                align={ai === 0 ? "start" : ai === model.actions.length - 1 ? "end" : "center"}
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
              {t("Delete")}
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
                  {fieldTitle(f)}
                  {f.required && (
                    <span className="req" title={t("Required")} aria-hidden="true">
                      *
                    </span>
                  )}
                  {f.personal_data && (
                    <span
                      className="shield"
                      title={t("Personal data ({class}). Agents see it masked.", {
                        class: f.class,
                      })}
                      role="img"
                      aria-label={t("Personal data, {class}", { class: f.class })}
                    >
                      <Icon name="shield" size={13} />
                    </span>
                  )}
                  {f.class === "financial" && <span className="chip">{t("financial")}</span>}
                </label>
                {isMasked_ ? (
                  <div className="maskedfield">
                    <span className="masked" id={fid}>
                      {String(row?.[f.name])}
                    </span>
                    <span className="hint">{t("Masked for this session")}</span>
                    {canEdit && (
                      <button
                        type="button"
                        className="linkbtn"
                        onClick={() => setDraft({ ...draft, [f.name]: "" })}
                      >
                        {t("Replace")}
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
                      if (out.kind === "ok")
                        d.notify(
                          t("{field}: {values}", {
                            field: fieldTitle(f),
                            values: optionLabel(to, f),
                          }),
                        );
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
                    altHint={(() => {
                      const alt = model.fields.find((x) => x.name === `${f.name}_alt`);
                      return alt
                        ? { title: fieldTitle(alt), empty: !String(value(alt)).trim() }
                        : undefined;
                    })()}
                    currency={currency}
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
                      {rich("Open {name}", {
                        name: <RefLabel field={f} id={String(value(f))} link={false} />,
                      })}
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
            <Icon name="shield" size={13} />{" "}
            {t("Withheld from you: {fields}", {
              fields: withheld
                .map((w) => fieldTitleByName(w.split(".")[1] ?? w, fields))
                .join(", "),
            })}
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
            {creating ? t("Create") : t("Save")}
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
            {dirty ? t("Discard") : t("Close")}
          </Button>
          {!canEdit && <span className="hint">{t("Read only for you")}</span>}
          <span className="grow" />
          <span className="hint kbd-hint">
            {rich("<kbd>⌘S</kbd> save <kbd>Esc</kbd> close", {}, { kbd: (c) => <Kbd>{c}</Kbd> })}
          </span>
        </footer>
      </form>
      {deleting && row && (
        <DeleteDialog
          model={model}
          noun={inSentence(title)}
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
          ariaLabel={t("Move {field}", { field: fieldTitle(f) })}
          triggerClassName="btn small"
          trigger={<>{t("Move to…")}</>}
          items={(f.options ?? [])
            .filter((o) => o !== current)
            .map((o) => {
              const mv = canMove(model, f, current, o, p.title);
              return {
                label: `${optionLabel(o, f)}${mv.needsApproval ? " *" : ""}`,
                disabled: p.busy || !mv.allowed,
                description: mv.reason || undefined,
                onSelect: () => p.onMove(o),
              };
            })}
        />
      ) : (
        <span className="hint">
          {t("Starts as {state}; it moves on through actions.", {
            state: optionLabel(initialOption(f), f),
          })}
        </span>
      )}
    </div>
  );
}
