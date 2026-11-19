import { useEffect, useRef, useState } from "react";
import { Badge, Button, Dialog, Notice, Select, Table, fmt, inSentence, t } from "@obop/ui";
import { call, type ModelMeta } from "./api.ts";
import { useData } from "./data.tsx";
import { csvCell, download } from "./format.ts";
import {
  MAX_BYTES,
  lookupColumns,
  overLimit,
  readSheet,
  resolveNames,
  sameRefusal,
  shortName,
  targetOf,
  titleReads,
  type Look,
  type Problem,
  type Target,
} from "./importNames.ts";
import { fieldTitle, modelTitle } from "./labels.ts";
import { serverMessage } from "./serverMessage.ts";

/* D70: import records of a model from a CSV file through bulk_create. The file is read in the
   browser and sent as text; the server parses it and checks every row exactly as it would check a
   record created by hand. A dry run shows the problems first; nothing is written until the person
   confirms. A fresh idempotency key per chosen file means a double click cannot import twice.
   A reference column may hold the other record's name (D70 amendment, importNames.ts): the names
   are looked up once per chosen file, and the check and the import send the same text. */

/** The most the server takes in one call (kept in step with bulk_create). */
const MAX_ROWS = 1000;

type Mode = "all_or_nothing" | "partial";
type RowError = { row: number; line?: number; error: string };
type Report = {
  ok: boolean;
  total: number;
  valid?: number;
  created?: number;
  failed: number;
  errors: RowError[];
  errors_truncated?: boolean;
};

type Phase =
  | { at: "choose" }
  | { at: "resolving" }
  | { at: "checking" }
  | { at: "checked"; report: Report }
  | { at: "refused"; word: () => string }
  | { at: "importing" }
  | { at: "proposed" }
  | { at: "done"; report: Report };

export function ImportDialog({
  model,
  title,
  onClose,
}: {
  model: ModelMeta;
  /** The model's name for people ("Contacts"). */
  title: string;
  onClose: () => void;
}) {
  const d = useData();
  const [phase, setPhase] = useState<Phase>({ at: "choose" });
  const [mode, setMode] = useState<Mode>("all_or_nothing");
  const [fileName, setFileName] = useState("");
  const csv = useRef("");
  const key = useRef("");
  /** The browser's own sentence for a row whose reference it could not look up, by row. */
  const problems = useRef(new Map<number, Problem>());
  const input = useRef<HTMLInputElement>(null);
  /** Counts the chosen files and the checks: an answer that lands after a newer one is dropped. */
  const turn = useRef(0);
  /** What the dialog found: the report, or why the file cannot be imported. */
  const result = useRef<HTMLDivElement>(null);
  const columns = model.fields
    .filter((f) => !f.computed && !f.sequence && f.kind !== "image" && f.kind !== "file")
    .map((f) => f.name);
  /** Fields and their columns, as the "Fields you can fill" line lists them. */
  const listed = (names: readonly string[]) =>
    names
      .map((c) => {
        const f = model.fields.find((x) => x.name === c);
        // A no-break space, so a line never ends between a field's name and its column.
        return f ? `${fieldTitle(f)}\u00a0(${c})` : c;
      })
      .join(", ");
  const refs = columns.filter((c) => model.fields.some((f) => f.name === c && f.kind === "ref"));

  async function check(text: string, m: Mode) {
    const mine = ++turn.current;
    setPhase({ at: "checking" });
    const r = await call(d.token, `${model.name}.bulk_create`, {
      csv: text,
      mode: m,
      dry_run: true,
    });
    if (mine !== turn.current) return;
    if (r.kind === "ok") setPhase({ at: "checked", report: r.value as Report });
    else if (r.kind === "approval") setPhase({ at: "proposed" });
    else setPhase({ at: "refused", word: () => serverMessage(r.message) });
  }

  /**
   * The text to send for the person's file: the same string when no cell needs a lookup, else the
   * file with each name found once replaced by its record's id, and the problems of the rest.
   * `null`: with the ids in it, the file is over the server's limit.
   */
  async function prepare(
    raw: string,
  ): Promise<{ text: string; problems: Map<number, Problem> } | null> {
    const sheet = readSheet(raw);
    const wanted = lookupColumns(sheet, model);
    if (!sheet || wanted.length === 0) return { text: raw, problems: new Map() };
    setPhase({ at: "resolving" });
    const look: Look = {
      model: (n) => d.models.get(n)?.model,
      rows: (n) => d.store.cached(n),
    };
    // A model the person may not read is left to the server, which says there is no such record.
    const tos = [...new Set(wanted.map((c) => c.to))].filter((to) => d.models.has(to));
    const reads = new Map(
      tos.map((to) => [to, [to, ...titleReads(d.models.get(to)!.model, look)]] as const),
    );
    const load = new Set([...reads.values()].flat().filter((m) => d.models.has(m)));
    // Fresh rows, once per chosen file: rows a view holds are loaded again (a load of them already
    // running is followed by a new one); a model with no rows yet joins a load already running,
    // which was asked for before the file was chosen (RowStore.load).
    await Promise.all(
      [...load].map((m) => (d.store.cached(m) ? d.store.refresh(m) : d.store.load(m))),
    );
    const targets = new Map<string, Target | null>();
    for (const to of tos) {
      if (d.store.errorOf(to)) continue;
      // Complete only when every record was loaded: the total is counted before the list is capped.
      const complete = reads
        .get(to)!
        .filter((m) => d.models.has(m))
        .every(
          (m) => !d.store.errorOf(m) && d.store.totalOf(m) <= (d.store.cached(m)?.length ?? 0),
        );
      const target = d.models.get(to)!.model;
      targets.set(
        to,
        targetOf(target, look, (r) => d.recordLabel(to, r), complete),
      );
    }
    const out = resolveNames(raw, sheet, wanted, targets);
    // readSheet gave up on a text over the limit as read (the server's to refuse), so only the ids
    // can have made this one larger.
    return overLimit(out.text) ? null : out;
  }

  async function pick(file: File | undefined) {
    if (!file) return;
    // A file chosen while an earlier one is still read or looked up wins: the earlier one stops.
    const mine = ++turn.current;
    setFileName(file.name);
    // No report of the earlier file, so nothing can be imported until this one is checked.
    setPhase({ at: "choose" });
    csv.current = "";
    problems.current = new Map();
    if (file.size > MAX_BYTES) {
      setPhase({
        at: "refused",
        word: () => t("The file is larger than 1 MiB. Split it into smaller files."),
      });
      return;
    }
    const raw = await file.text();
    if (mine !== turn.current) return;
    const ready = await prepare(raw);
    if (mine !== turn.current) return;
    if (!ready) {
      setPhase({
        at: "refused",
        word: () =>
          t(
            "With the names looked up, the file is larger than 1 MiB. Split it into smaller files.",
          ),
      });
      return;
    }
    // The text and its key are made together: the check and the import send this text.
    csv.current = ready.text;
    problems.current = ready.problems;
    key.current = crypto.randomUUID();
    await check(csv.current, mode);
  }

  /** The browser's sentence for a reference it left as a name. */
  function problemText(p: Problem): string {
    const info = d.models.get(p.to);
    const args = {
      model: inSentence(info ? modelTitle(info.model, info.module.id) : p.to.replace(/_/g, " ")),
      name: shortName(p.text),
      column: p.header,
    };
    switch (p.kind) {
      case "none":
        return t(
          "No {model} is named “{name}” (column {column}). Check the spelling, or add it first.",
          args,
        );
      case "several":
        return t(
          "More than one {model} is named “{name}” (column {column}). Give one of them another name, or write the id of the one you mean: open it and copy the text after “id=” from the address bar.",
          args,
        );
      case "archived":
        return t(
          "The only {model} named “{name}” is archived (column {column}). Restore it first, or choose another.",
          args,
        );
      case "unsure":
        return t(
          "The import cannot be sure which {model} “{name}” means (column {column}). Write its id instead: open the record and copy the text after “id=” from the address bar.",
          args,
        );
    }
  }

  async function run() {
    setPhase({ at: "importing" });
    const r = await d.run(`${model.name}.bulk_create`, {
      csv: csv.current,
      mode,
      idempotency_key: key.current,
    });
    if (r.kind === "ok") setPhase({ at: "done", report: r.value as Report });
    else if (r.kind === "approval") setPhase({ at: "proposed" });
    else setPhase({ at: "refused", word: () => serverMessage(r.message) });
  }

  const checked = phase.at === "checked" ? phase.report : null;
  const canImport = !!checked && (mode === "partial" ? (checked.valid ?? 0) > 0 : checked.ok);
  const busy = phase.at === "resolving" || phase.at === "checking" || phase.at === "importing";
  const report = phase.at === "done" ? phase.report : checked;
  const outcome = !!report || phase.at === "refused" || phase.at === "proposed";

  // What the dialog found is below the hints, out of sight on a phone: bring it into view. The
  // control that chose the file was disabled while the dialog worked, so the focus is moved to
  // the result too, and not left on the Close button where Enter would close the dialog.
  useEffect(() => {
    const el = result.current;
    if (!el) return;
    el.scrollIntoView?.({ block: "nearest" });
    const at = document.activeElement;
    if (!at || !el.closest(".import-dialog")?.contains(at)) el.focus({ preventScroll: true });
  }, [phase]);

  return (
    <Dialog
      title={t("Import {model} from a CSV file", { model: title })}
      size="lg"
      onClose={busy ? () => {} : onClose}
      footer={
        phase.at === "done" || phase.at === "proposed" ? (
          <Button variant="primary" onClick={onClose}>
            {t("Done")}
          </Button>
        ) : (
          <>
            <Button onClick={onClose} disabled={busy}>
              {t("Cancel")}
            </Button>
            <Button
              variant="primary"
              loading={phase.at === "importing"}
              disabled={!canImport}
              onClick={() => void run()}
            >
              {checked && canImport
                ? t("Import {count} rows", { count: fmt.integer(checked.valid ?? checked.total) })
                : t("Import")}
            </Button>
          </>
        )
      }
    >
      <div className="stack import-dialog">
        {phase.at !== "done" && phase.at !== "proposed" && (
          <>
            <p className="hint">
              {t(
                "The first line of the file names the fields. Number fields take whole numbers; money is in minor units such as cents. Up to {rows} rows and 1 MiB, UTF-8, comma or semicolon.",
                { rows: fmt.integer(MAX_ROWS) },
              )}
            </p>
            {refs.length > 0 && (
              <p className="hint">
                {t(
                  "A column that points to another record ({columns}) takes the name of that record, written as the lists show it. Capital letters and extra spaces do not matter. If two records have the same name, give one of them another name first.",
                  { columns: listed(refs) },
                )}
              </p>
            )}
            {model.fields.some((f) => f.kind === "decimal" || f.kind === "datetime") && (
              <p className="hint">
                {t(
                  "Decimal numbers are written with a point and no thousands separators (12.50), never with an exponent. A date and time ends with Z or an offset (2026-03-29T01:30:00Z or 2026-03-29T03:30:00+02:00). Too many decimals are refused unless the field rounds when importing.",
                )}
              </p>
            )}
            <p className="hint">
              {t("Fields you can fill: {fields}", { fields: listed(columns) })}
            </p>
            <div className="import-pick">
              <input
                ref={input}
                type="file"
                accept=".csv,text/csv"
                className="sr-only"
                aria-label={t("Choose a CSV file")}
                disabled={busy}
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  // Emptied, so the same file chosen again (after it was corrected) is read again.
                  e.target.value = "";
                  void pick(file);
                }}
              />
              <Button icon="upload" disabled={busy} onClick={() => input.current?.click()}>
                {fileName ? t("Choose another file") : t("Choose a CSV file")}
              </Button>
              {!fileName && (
                // An empty file with the right first line: fill it in a spreadsheet and pick it.
                <Button
                  variant="ghost"
                  icon="download"
                  onClick={() =>
                    download(`${model.name}-template.csv`, `${columns.map(csvCell).join(",")}\r\n`)
                  }
                >
                  {t("Download a template")}
                </Button>
              )}
              {fileName && <span className="mono import-file">{fileName}</span>}
            </div>
            <Select
              label={t("If a row is wrong")}
              value={mode}
              disabled={busy}
              onChange={(e) => {
                const m = e.target.value as Mode;
                setMode(m);
                if (csv.current) void check(csv.current, m);
              }}
            >
              <option value="all_or_nothing">{t("Stop: import nothing")}</option>
              <option value="partial">{t("Import the good rows and report the rest")}</option>
            </Select>
          </>
        )}
        {phase.at === "resolving" && (
          <p role="status">{t("Looking up the names in your file...")}</p>
        )}
        {phase.at === "checking" && <p role="status">{t("Checking the file...")}</p>}
        {phase.at === "importing" && <p role="status">{t("Importing...")}</p>}
        {outcome && (
          <div ref={result} tabIndex={-1} className="stack import-result">
            {phase.at === "refused" && (
              <Notice tone="danger" title={t("The file cannot be imported")}>
                {phase.word()}
              </Notice>
            )}
            {phase.at === "proposed" && (
              <Notice tone="info" title={t("Proposed, not imported")}>
                {t("A person with the right to create these records must approve it first.")}
              </Notice>
            )}
            {report && (
              <div className="stack">
                <p role="status" className="import-summary">
                  {phase.at === "done" ? (
                    <>
                      <Badge tone="success">{t("Imported")}</Badge>{" "}
                      {t("Created: {created}. Skipped: {failed}.", {
                        created: fmt.integer(report.created ?? 0),
                        failed: fmt.integer(report.failed),
                      })}
                    </>
                  ) : (
                    <>
                      <Badge tone={report.failed === 0 ? "success" : "warning"}>
                        {report.failed === 0 ? t("Ready") : t("Problems")}
                      </Badge>{" "}
                      {t("Rows ready: {valid}. Rows with problems: {failed}.", {
                        valid: fmt.integer(report.valid ?? 0),
                        failed: fmt.integer(report.failed),
                      })}
                    </>
                  )}
                </p>
                {report.errors.length > 0 && (
                  // One block per problem on a phone, the message on top (stack). The explicit roles
                  // keep it a table for Safari, which drops the semantics once display changes.
                  <Table caption={t("Problems found")} dense stack role="table">
                    <thead>
                      <tr role="row">
                        <th scope="col" role="columnheader">
                          {t("Row")}
                        </th>
                        <th scope="col" role="columnheader">
                          {t("Line")}
                        </th>
                        <th scope="col" role="columnheader">
                          {t("Problem")}
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {report.errors.map((e) => (
                        <tr key={e.row} role="row">
                          <td role="cell" data-label={t("Row")}>
                            {e.row}
                          </td>
                          <td role="cell" data-label={t("Line")}>
                            {e.line ?? ""}
                          </td>
                          <td role="cell" className="cell-main">
                            <ProblemCell
                              ours={problems.current.get(e.row)}
                              error={e.error}
                              text={problemText}
                            />
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </Table>
                )}
                {report.errors_truncated && (
                  <p className="hint">{t("Only the first 100 problems are listed.")}</p>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </Dialog>
  );
}

/**
 * The Problem cell: the browser's sentence for a reference it left as a name, and the server's
 * own sentence after it on a second line when that one is about something else, so nothing the
 * server says about the row is hidden.
 */
function ProblemCell({
  ours,
  error,
  text,
}: {
  ours: Problem | undefined;
  error: string;
  text: (p: Problem) => string;
}) {
  if (!ours) return <>{serverMessage(error)}</>;
  if (sameRefusal(ours, error)) return <>{text(ours)}</>;
  return (
    <>
      {text(ours)}
      <br />
      {serverMessage(error)}
    </>
  );
}
