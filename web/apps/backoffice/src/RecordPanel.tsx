// The record panel: the state of one record's form and the commands it runs. What it draws is in
// RecordHeader.tsx (title, summary, status, actions), RelatedChips.tsx and FormBody.tsx (F2 of the
// UI plan); what an app adds to it is registered in extensions.ts. Lane L5 (plan 2.6): the header
// from presentation.ts, actions that save first and ask what they need, sections, the full page,
// the pager and the trail, locks and read-only values.
import { useEffect, useMemo, useRef, useState } from "react";
import { getCurrency, inSentence, t, useFocusTrap } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, Row } from "./api.ts";
import { ActionDialog } from "./ActionDialog.tsx";
import { useData } from "./data.tsx";
import { fromEdit, sameAsStored, toEdit, validate, type Edit } from "./edit.ts";
import { recordExtensionOf, withoutOverridden } from "./extensions.ts";
import { FormBody, flushLines, linesDirty, newLinesRegistry } from "./FormBody.tsx";
import { isMasked, isState, optionLabel, rowCurrency } from "./format.ts";
import { actionTitle, fieldTitle } from "./labels.ts";
import { extendTrail, trailStore } from "./lastRecord.ts";
import { serverMessage } from "./serverMessage.ts";
import { DeleteDialog } from "./DeleteDialog.tsx";
import { PHONE_QUERY, SHEET_QUERY, useMedia, useScrollLock } from "./media.ts";
import {
  indexModels,
  currencyOf,
  newDefaults,
  nextActions,
  subtitleOf,
  titleOf,
  type NextActions,
  type PresentCtx,
} from "./presentation.ts";
import {
  actionLabelFor,
  avatarField,
  copyValues,
  createdRefs,
  deleteBlocked,
  keepShown,
  keyFigures,
  lockOf,
  sectionsOf,
  serverSet,
} from "./recordLayout.ts";
import { RecordHeader } from "./RecordHeader.tsx";
import { RelatedChips } from "./RelatedChips.tsx";
import { nav } from "./route.ts";
import "./registrations.ts";
import { initialOption, isGoverned, moveRecord } from "./workflow.ts";

type Props = {
  model: ModelMeta;
  fields: FieldMeta[];
  row: Row | null;
  /** The values a new record starts with (the page's context); presentation's defaults fill the rest. */
  defaults: Record<string, unknown>;
  withheld: string[];
  title: string;
  onClose: () => void;
  onSaved: (id: string) => void;
  /** The ids of the list the record was opened from, in its order, for the pager. */
  siblings?: readonly string[];
  /** Open another record of the same model (the pager). */
  onOpen?: (id: string) => void;
  /** Whether the record fills the page (`&full=1`), and how to switch. */
  full?: boolean;
  onFull?: (on: boolean) => void;
  /** Open a new record with these values (Duplicate). */
  onDuplicate?: (values: Record<string, unknown>) => void;
};

const without = <V,>(m: Record<string, V>, name: string): Record<string, V> =>
  Object.fromEntries(Object.entries(m).filter(([k]) => k !== name));

const NO_ACTIONS: NextActions = { secondary: [], danger: [] };

/**
 * The open panels, the last opened last: a record created from a picker ("Create and edit…")
 * opens a second panel over the first, and only the top one answers a key pressed outside both.
 */
const openPanels: object[] = [];

/** A control that owns the arrow keys (a select opens on Alt+Down, a text moves its caret). */
const OWNS_KEYS = "input, select, textarea, [contenteditable], [role=combobox], [role=listbox]";

/**
 * A record link whose record is gone or hidden from the person says so and drops the id, instead
 * of showing the list without a word (B17). `found` is what `useRecord` answered: null when the
 * server did not return the record.
 */
export function useMissingRecord(found: Row | null | undefined, id: string, close: () => void) {
  const d = useData();
  const closeRef = useRef(close);
  closeRef.current = close;
  useEffect(() => {
    if (!id || id === "new" || found !== null) return;
    d.notify(t("This record no longer exists, or you may not see it"), "error");
    closeRef.current();
  }, [found, id]);
}

/** The record opens in a floating side panel, or as a page (docs/spec/ux.md 4.1). */
export function RecordPanel({
  model,
  fields,
  row,
  defaults: given,
  withheld,
  title,
  onClose,
  onSaved,
  siblings,
  onOpen,
  full = false,
  onFull,
  onDuplicate,
}: Props) {
  const d = useData();
  const creating = row === null;
  const [draft, setDraft] = useState<Record<string, Edit>>({});
  // What went wrong is kept as names and as the server's own words, and the sentence is written
  // when it is drawn: text kept finished would stay in the language it was written in after a
  // change of language (the panel keeps its state through one).
  const [invalid, setInvalid] = useState<Record<string, true>>({}); // failed the check at Save
  const [refused, setRefused] = useState<Record<string, string>>({}); // the server's words, by field
  const [error, setError] = useState<{ raw: string } | null>(null); // the server's words, for the form
  const clearField = (name: string) => {
    setInvalid((m) => without(m, name));
    setRefused((m) => without(m, name));
  };
  const [busy, setBusy] = useState(false);
  const [deleting, setDeleting] = useState(false);
  // An action waiting for its question to be answered (ActionDialog).
  const [asking, setAsking] = useState<Command | null>(null);
  // What the last action made, to open it ("Draft invoice created · Open").
  const [made, setMade] = useState<{ field: FieldMeta; id: string }[]>([]);
  const panel = useRef<HTMLElement>(null);
  // Up to 1024 px the record is a sheet over the list, full-screen on a phone (D2).
  const sheet = useMedia(SHEET_QUERY);
  const phone = useMedia(PHONE_QUERY);
  useFocusTrap(panel, sheet);
  useScrollLock(sheet);
  const statusField = fields.find(isState);
  const canEdit = creating ? model.can_create : model.can_update;
  // What the model's registrations add (extensions.ts): the Sign app's signing links.
  const ext = recordExtensionOf(model.name);
  const moduleId = d.models.get(model.name)?.module.id ?? "";
  const index = useMemo(() => indexModels(d.modules), [d.modules]);
  const ctx: PresentCtx = {
    refText: d.refText,
    rowOf: (m, id) => d.store.cached(m)?.find((r) => r.id === id),
    modelOf: (m) => index.get(m),
    moduleId,
    instanceCurrency: getCurrency(),
  };

  // A new record starts with the page's values over the presentation's defaults (today for a
  // required date, the instance's currency, the hints of the package).
  const defaults = useMemo(
    () =>
      creating
        ? newDefaults({ ...model, fields }, { instanceCurrency: getCurrency(), route: given })
        : given,
    // The panel is keyed by the record and its defaults: they are read once.
    [],
  );

  // A governed field (D48) starts at its first option and then moves only through actions.
  const governed = (f: FieldMeta) => isGoverned(model, f);
  // The record's own currency, else the one its journal or company is in (`ui.currency_from`).
  const currency =
    rowCurrency(row ?? defaults) ??
    currencyOf({ id: row?.id ?? "", ...(row ?? defaults) } as Row, model, {
      ...ctx,
      rowOf: (m, id) => d.store.peek(m)?.find((r) => r.id === id),
    });
  const baseline = (f: FieldMeta): Edit =>
    creating && governed(f)
      ? initialOption(f)
      : toEdit(f, row ? row[f.name] : defaults[f.name], currency);
  const value = (f: FieldMeta): Edit => (f.name in draft ? draft[f.name]! : baseline(f));
  const masked = (f: FieldMeta) => !!row && isMasked(row[f.name]) && !(f.name in draft);
  // An unset switch shows No: choosing No for it is a change that writes an explicit false.
  const changed = (f: FieldMeta) =>
    f.name in draft &&
    (draft[f.name] !== baseline(f) ||
      (!!row && f.kind === "bool" && !sameAsStored(f, row[f.name], draft[f.name]!)));
  const dirty = fields.some(changed);
  // Unsaved line edits count as unsaved changes: leaving asks first, and Save saves them too.
  const lines = useRef(newLinesRegistry()).current;
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;
  const unsaved = () => dirtyRef.current || linesDirty(lines);
  // The save that is on its way: leaving waits for it instead of asking about it (`nav.saving`).
  const saving = useRef<Promise<void> | null>(null);

  // A frozen record (meta `guard`) keeps only its mutable fields; the server says the same.
  const lock = lockOf(model, row);
  const fixed = (f: FieldMeta) => !canEdit || serverSet(f) || (!!lock && !lock.mutable.has(f.name));
  const editable = fields.some((f) => !fixed(f) && !governed(f) && f !== statusField);
  const writable = (f: FieldMeta) => !fixed(f) && !masked(f);

  const label = row ? titleOf(model, row, ctx) : "";
  const figures = keyFigures(model, fields, row);
  const shownInHeader = new Set(figures.filter((f) => f.computed).map((f) => f.name));
  const bodyFields = fields.filter(
    (f) => f !== statusField && !(creating && serverSet(f)) && !shownInHeader.has(f.name),
  );
  const shownSections = useRef(new Set<string>()).current;
  const sections = keepShown(
    sectionsOf(model, bodyFields, {
      creating,
      row,
      state: statusField ? String(value(statusField)) : undefined,
      filled: (f) => {
        const v = value(f);
        return v !== "" && v !== false;
      },
    }),
    shownSections,
  );
  const chips = row ? subtitleOf(model, row, ctx).split(" · ").filter(Boolean).slice(0, 3) : [];
  const actions = row ? withoutOverridden(nextActions(model, row), ext.overrides) : NO_ACTIONS;
  const mayDelete = !!row && model.can_delete && !deleteBlocked(model, row);

  // The trail of records: a link followed inside this panel makes the next panel a step deeper.
  const key = row ? `${model.name}:${row.id}` : "";
  const [crumbs, setCrumbs] = useState(trailStore.trail);
  useEffect(() => {
    if (!row) return;
    const href = d.linkTo(model.name, { id: row.id }) ?? "";
    trailStore.trail = extendTrail(trailStore.trail, { key, label, href }, trailStore.from);
    trailStore.from = null;
    setCrumbs(trailStore.trail);
    // The label is read again when it changes (a number assigned); the trail is made once.
  }, [key]);
  useEffect(() => {
    if (!row || !label) return;
    trailStore.trail = trailStore.trail.map((c) => (c.key === key ? { ...c, label } : c));
    setCrumbs(trailStore.trail);
  }, [label]);
  const shownCrumbs = crumbs.length > 1 && crumbs[crumbs.length - 1]?.key === key ? crumbs : [];

  // The pager: the neighbours in the list's order.
  const at = row && siblings ? siblings.indexOf(row.id) : -1;
  const pager =
    at >= 0 && siblings && siblings.length > 1 && onOpen
      ? {
          index: at,
          total: siblings.length,
          prev: at > 0 ? () => onOpen(siblings[at - 1]!) : undefined,
          next: at < siblings.length - 1 ? () => onOpen(siblings[at + 1]!) : undefined,
        }
      : undefined;
  const pagerRef = useRef(pager);
  pagerRef.current = pager;

  // Leaving with unsaved edits asks first (hash navigation, tab close).
  // A panel opened over another one keeps the one below it guarded too, and gives its guard back.
  useEffect(() => {
    const below = nav.dirty;
    const mine = () => unsaved() || !!below?.();
    nav.dirty = mine;
    const belowSaving = nav.saving;
    const mineSaving = () => saving.current ?? belowSaving?.() ?? null;
    nav.saving = mineSaving;
    const before = (e: BeforeUnloadEvent) => {
      if (unsaved()) e.preventDefault();
    };
    addEventListener("beforeunload", before);
    return () => {
      if (nav.dirty === mine) nav.dirty = below ?? null;
      if (nav.saving === mineSaving) nav.saving = belowSaving ?? null;
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
    const me = {};
    openPanels.push(me);
    const on = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return;
      // A key pressed in this panel is this panel's; one pressed elsewhere only the top panel's.
      const target = e.target instanceof Node ? e.target : null;
      const inside = !!target && !!panel.current?.contains(target);
      if (!inside && openPanels.at(-1) !== me) return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        if (linesDirty(lines)) void flushLines(lines);
        if (dirtyRef.current || creating) saveRef.current();
      } else if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
        // A select, a text or a picker keeps its own arrow keys (images reorder with them too).
        if (target instanceof Element && target.closest(OWNS_KEYS)) return;
        const go = e.key === "ArrowUp" ? pagerRef.current?.prev : pagerRef.current?.next;
        if (go) {
          e.preventDefault();
          go();
        }
      } else if (e.key === "Escape" && !document.querySelector(".overlay")) {
        onClose();
      }
    };
    addEventListener("keydown", on);
    return () => {
      removeEventListener("keydown", on);
      const i = openPanels.indexOf(me);
      if (i >= 0) openPanels.splice(i, 1);
    };
  }, [onClose, creating]);

  function failure(message: string) {
    const prefix = `${model.name}.`;
    const f = message.startsWith(prefix)
      ? fields.find((x) => message.startsWith(`${prefix}${x.name} `))
      : undefined;
    // The server's English is matched first, then shown in the person's language (when drawn).
    if (f) setRefused((e) => ({ ...e, [f.name]: message }));
    else setError({ raw: message });
  }

  /** Save or create; the id of the saved record, or null when nothing was saved. */
  async function save(): Promise<string | null> {
    setError(null);
    const editable = fields.filter(writable);
    const errs = validate(
      editable,
      value,
      creating ? undefined : (f) => changed(f) || f.required,
      currency,
      // A value left as stored is not held to limits lowered since it was written (D90).
      (f) => !creating && !changed(f),
    );
    setInvalid(Object.fromEntries(Object.keys(errs).map((name) => [name, true as const])));
    setRefused({});
    if (Object.keys(errs).length) {
      d.notify(t("Fix the highlighted fields first"), "error");
      return null;
    }
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
    // Until the server has answered, a way out (Escape, a link, Back) waits; it goes on when this
    // function is over, so after `onSaved` has put the saved record in the address.
    let over = () => {};
    const sent = new Promise<void>((done) => (over = done));
    const answered = () => {
      if (saving.current === sent) saving.current = null;
    };
    saving.current = sent;
    try {
      const r = creating
        ? await d.run(`${model.name}.create`, body)
        : await d.run(`${model.name}.update`, { id: row.id, ...body });
      answered();
      setBusy(false);
      if (r.kind === "ok") {
        d.notify(creating ? t("{model} created", { model: title }) : t("Saved"));
        setDraft({});
        dirtyRef.current = false;
        const id = String((r.value as Row).id ?? row?.id);
        onSaved(id);
        return id;
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
      return null;
    } finally {
      answered();
      over();
    }
  }

  async function setStatus(s: string) {
    if (!statusField || creating || busy) return;
    if (!governed(statusField) && !model.can_update) return;
    setBusy(true);
    setError(null);
    const out = await moveRecord(d.run, model, statusField, row, s, title);
    setBusy(false);
    if (out.kind === "ok") {
      setDraft((x) => without(x, statusField.name));
      d.notify(
        t("{field}: {values}", {
          field: fieldTitle(statusField),
          values: optionLabel(s, statusField),
        }),
      );
    } else if (out.kind === "approval") d.notify(out.message, "info");
    else {
      // The server's own words (D48), also inline so they stay readable.
      setError({ raw: out.raw });
      d.notify(out.message, "error");
    }
  }

  const needsOf = (a: Command): FieldMeta[] =>
    (a.ui?.needs ?? [])
      .map((n) => model.fields.find((f) => f.name === n))
      .filter((f): f is FieldMeta => !!f);

  /** An action button was pressed: ask first when it needs words or a confirm, else run it. */
  function requestAction(a: Command) {
    if (!row || busy) return;
    if (needsOf(a).length || a.ui?.confirm || actions.danger.includes(a)) setAsking(a);
    else void runAction(a, {});
  }

  /** Run an action; unsaved input is saved first, so nothing typed is lost (B4). */
  async function runAction(a: Command, extra: Record<string, unknown>) {
    if (!row) return;
    setAsking(null);
    if (dirtyRef.current && (await save()) === null) return;
    setBusy(true);
    setError(null);
    setMade([]);
    const r = await d.run(a.id, { id: row.id, ...extra });
    setBusy(false);
    const name = actionTitle(a);
    if (r.kind === "ok") {
      const after = (r.value ?? {}) as Record<string, unknown>;
      const f = a.field ? model.fields.find((x) => x.name === a.field) : undefined;
      const now = { ...row, ...after } as Row;
      d.notify(
        a.ui?.done ??
          (f && a.to
            ? t("{model} {name} is now {state}", {
                model: title,
                name: titleOf(model, now, ctx),
                state: inSentence(optionLabel(a.to, f)),
              })
            : t("{name} done", { name })),
      );
      setMade(createdRefs(model, row, after).map((x) => ({ field: x, id: String(after[x.name]) })));
    } else if (r.kind === "approval")
      d.notify(
        t(
          "Approval needed: “{name}” was proposed (plan {plan}). Nothing changes until a human approves it.",
          { name, plan: r.planId.slice(0, 8) },
        ),
        "info",
      );
    else {
      const message = serverMessage(r.message);
      setError({ raw: r.message });
      d.notify(
        a.needs_approval ? t("{name} was refused: {reason}", { name, reason: message }) : message,
        "error",
      );
    }
  }

  async function moveField(f: FieldMeta, to: string) {
    if (!row) return;
    if (dirtyRef.current && (await save()) === null) return;
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
      setError({ raw: out.raw });
      d.notify(out.message, "error");
    }
  }

  return (
    <>
      {sheet && <div className="record-scrim" aria-hidden="true" onClick={onClose} />}
      <aside
        className={`record panel${sheet ? " sheet" : ""}${phone ? " phone" : ""}${full ? " full" : ""}`}
        aria-label={t("{model} record", { model: title })}
        role={sheet ? "dialog" : undefined}
        aria-modal={sheet || undefined}
        ref={panel}
        tabIndex={-1}
        onClickCapture={(e) => {
          // A link to another record followed from here continues the trail.
          if (row && (e.target as HTMLElement).closest("a[href^='#/']")) trailStore.from = key;
        }}
      >
        <RecordHeader
          model={model}
          fields={fields}
          row={row}
          title={title}
          label={label}
          phone={phone}
          busy={busy}
          statusField={statusField}
          value={value}
          governed={governed}
          pickStatus={(s) => {
            if (statusField) setDraft({ ...draft, [statusField.name]: s });
          }}
          setStatus={(s) => void setStatus(s)}
          actions={actions}
          runAction={requestAction}
          extensions={ext}
          onDelete={mayDelete ? () => setDeleting(true) : undefined}
          onDuplicate={
            row && onDuplicate && model.can_create
              ? () => onDuplicate(copyValues(model, fields, row, governed))
              : undefined
          }
          onClose={onClose}
          between={!creating && <RelatedChips model={model} id={row.id} />}
          figures={figures}
          currency={currency}
          chips={chips}
          avatar={avatarField(fields)}
          crumbs={shownCrumbs}
          pager={pager}
          full={full}
          onFull={onFull}
          lock={lock}
        />
        {made.length > 0 && (
          <p className="rec-made" role="status">
            {made.map(({ field: f, id }) => {
              const href = f.to ? d.linkTo(f.to, { id }) : null;
              return (
                <span key={f.name}>
                  {t("{name} created", { name: fieldTitle(f) })}
                  {href && (
                    <>
                      {" · "}
                      <a href={href}>{t("Open")}</a>
                    </>
                  )}
                </span>
              );
            })}
          </p>
        )}
        {!creating &&
          ext.panels.map((Panel, i) => (
            <Panel key={i} model={model} fields={fields} row={row} busy={busy} />
          ))}
        <FormBody
          lines={lines}
          model={model}
          fields={fields}
          row={row}
          title={title}
          canEdit={canEdit}
          phone={phone}
          busy={busy}
          dirty={dirty}
          statusField={statusField}
          currency={currency}
          withheld={withheld}
          error={error}
          invalid={invalid}
          refused={refused}
          value={value}
          masked={masked}
          changed={changed}
          governed={governed}
          sections={sections}
          fixed={fixed}
          editable={editable}
          locked={!!lock}
          onEdit={(name, v) => setDraft((x) => ({ ...x, [name]: v }))}
          clearField={clearField}
          onMove={(f, to) => void moveField(f, to)}
          onSubmit={() => void save()}
          onDiscard={() => {
            if (dirty) {
              setDraft({});
              setInvalid({});
              setRefused({});
              setError(null);
            } else onClose();
          }}
          save={save}
        />
        {asking && (
          <ActionDialog
            action={asking}
            label={actionLabelFor(asking, inSentence(title))}
            needs={needsOf(asking)}
            question={
              asking.ui?.confirm ??
              (actions.danger.includes(asking)
                ? t("{action} “{name}”?", {
                    action: actionLabelFor(asking, inSentence(title)),
                    name: label,
                  })
                : undefined)
            }
            danger={actions.danger.includes(asking)}
            currency={currency}
            onCancel={() => setAsking(null)}
            onConfirm={(values) => void runAction(asking, values)}
          />
        )}
        {deleting && row && (
          <DeleteDialog
            model={model}
            noun={inSentence(title)}
            records={[{ id: row.id, label: label || d.recordLabel(model.name, row) }]}
            onClose={() => setDeleting(false)}
            onDone={() => onClose()}
          />
        )}
      </aside>
    </>
  );
}
