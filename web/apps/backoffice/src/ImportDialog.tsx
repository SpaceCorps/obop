import { useRef, useState } from "react";
import { Badge, Button, Dialog, Notice, Select, Table, fmt, t } from "@obop/ui";
import { call, type ModelMeta } from "./api.ts";
import { useData } from "./data.tsx";
import { fieldTitle } from "./labels.ts";
import { serverMessage } from "./serverMessage.ts";

/* D70: import records of a model from a CSV file through bulk_create. The file is read in the
   browser and sent as text; the server parses it and checks every row exactly as it would check a
   record created by hand. A dry run shows the problems first; nothing is written until the person
   confirms. A fresh idempotency key per chosen file means a double click cannot import twice. */

/** The most the server takes in one call (kept in step with bulk_create). */
const MAX_ROWS = 1000;
const MAX_BYTES = 1_048_576;

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
  | { at: "checking" }
  | { at: "checked"; report: Report }
  | { at: "refused"; message: string }
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
  const input = useRef<HTMLInputElement>(null);
  const columns = model.fields
    .filter((f) => !f.computed && !f.sequence && f.kind !== "image" && f.kind !== "file")
    .map((f) => f.name);

  async function check(text: string, m: Mode) {
    setPhase({ at: "checking" });
    const r = await call(d.token, `${model.name}.bulk_create`, {
      csv: text,
      mode: m,
      dry_run: true,
    });
    if (r.kind === "ok") setPhase({ at: "checked", report: r.value as Report });
    else if (r.kind === "approval") setPhase({ at: "proposed" });
    else setPhase({ at: "refused", message: serverMessage(r.message) });
  }

  async function pick(file: File | undefined) {
    if (!file) return;
    setFileName(file.name);
    if (file.size > MAX_BYTES) {
      setPhase({
        at: "refused",
        message: t("The file is larger than 1 MiB. Split it into smaller files."),
      });
      return;
    }
    csv.current = await file.text();
    key.current = crypto.randomUUID();
    await check(csv.current, mode);
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
    else setPhase({ at: "refused", message: serverMessage(r.message) });
  }

  const checked = phase.at === "checked" ? phase.report : null;
  const canImport = !!checked && (mode === "partial" ? (checked.valid ?? 0) > 0 : checked.ok);
  const busy = phase.at === "checking" || phase.at === "importing";
  const report = phase.at === "done" ? phase.report : checked;

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
                "The first line of the file names the fields. References are the id of the record. Whole numbers only; money is in minor units such as cents. Up to {rows} rows and 1 MiB, UTF-8, comma or semicolon.",
                { rows: fmt.integer(MAX_ROWS) },
              )}
            </p>
            <p className="hint">
              {t("Fields you can fill: {fields}", {
                fields: columns
                  .map((c) => {
                    const f = model.fields.find((x) => x.name === c);
                    return f ? `${fieldTitle(f)} (${c})` : c;
                  })
                  .join(", "),
              })}
            </p>
            <div className="import-pick">
              <input
                ref={input}
                type="file"
                accept=".csv,text/csv"
                className="sr-only"
                aria-label={t("Choose a CSV file")}
                onChange={(e) => void pick(e.target.files?.[0])}
              />
              <Button icon="upload" disabled={busy} onClick={() => input.current?.click()}>
                {fileName ? t("Choose another file") : t("Choose a CSV file")}
              </Button>
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
        {phase.at === "checking" && <p role="status">{t("Checking the file...")}</p>}
        {phase.at === "importing" && <p role="status">{t("Importing...")}</p>}
        {phase.at === "refused" && (
          <Notice tone="danger" title={t("The file cannot be imported")}>
            {phase.message}
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
              <Table caption={t("Problems found")} dense>
                <thead>
                  <tr>
                    <th scope="col">{t("Row")}</th>
                    <th scope="col">{t("Line")}</th>
                    <th scope="col">{t("Problem")}</th>
                  </tr>
                </thead>
                <tbody>
                  {report.errors.map((e) => (
                    <tr key={e.row}>
                      <td>{e.row}</td>
                      <td>{e.line ?? ""}</td>
                      <td>{serverMessage(e.error)}</td>
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
    </Dialog>
  );
}
