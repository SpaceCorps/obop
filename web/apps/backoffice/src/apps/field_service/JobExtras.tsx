// Field Service on a job: the customer's sign-off and the printed job report.
// - "Customer signs" (a job in progress that needs a sign-off and has none): the customer types
//   their name and taps Sign, which writes `signed_by` and `signed_on` (today) with one
//   `field_service_job.update`. The state is not touched: Complete stays its own action, and the
//   server refuses it while the sign-off is missing (the `signed` assertion of `complete`).
// - "Print report": the job, the site, the parts used, the work report and the sign-off on paper
//   (a PrintSheet, shown on paper only). It reads `field_service_part.search`.
import { useEffect, useState } from "react";
import { Button, Dialog, Input, t } from "@obop/ui";
import { call, type Row } from "../../api.ts";
import { useData } from "../../data.tsx";
import type { RecordExtProps } from "../../extensions.ts";
import { formatDate, isoDate, money, optionLabel } from "../../format.ts";
import { clockText, shownNumber } from "../../inputs/display.ts";
import { serverMessage } from "../../serverMessage.ts";
import { PrintSheet } from "../sales/PrintSheet.tsx";
import { JOB, PART, needsSignature, siteText } from "./job.ts";
import "../sales/sales.css";
import "./field_service.css";

const str = (v: unknown) => (typeof v === "string" ? v : "");
const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : 0);

/** The customer signs the job off: name and today's date. */
export function SignOff({ row, busy }: RecordExtProps) {
  const d = useData();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [working, setWorking] = useState(false);
  if (!needsSignature(row) || !d.models.get(JOB)?.model.can_update) return null;

  const sign = async () => {
    const who = name.trim();
    if (!who) return;
    setWorking(true);
    try {
      const r = await d.run(`${JOB}.update`, {
        id: row.id,
        signed_by: who,
        signed_on: isoDate(new Date()),
      });
      if (r.kind === "ok") {
        setOpen(false);
        d.notify(t("Signed by {name}", { name: who }), "ok");
      } else if (r.kind === "approval") {
        setOpen(false);
        d.notify(t("Sent for approval"), "info");
      } else d.notify(serverMessage(r.message), "error");
    } finally {
      setWorking(false);
    }
  };

  return (
    <>
      <Button
        icon="edit"
        variant="primary"
        disabled={busy}
        onClick={() => {
          setName(str(row.site_contact_name));
          setOpen(true);
        }}
      >
        {t("Customer signs")}
      </Button>
      {open && (
        <Dialog
          open
          size="sm"
          onClose={() => !working && setOpen(false)}
          title={t("Sign off the work")}
          footer={
            <>
              <Button onClick={() => setOpen(false)} disabled={working}>
                {t("Cancel")}
              </Button>
              <Button
                variant="primary"
                loading={working}
                disabled={!name.trim()}
                onClick={() => void sign()}
              >
                {t("Sign")}
              </Button>
            </>
          }
        >
          <form
            className="fs-sign"
            onSubmit={(e) => {
              e.preventDefault();
              void sign();
            }}
          >
            <p className="fs-sign-job">{str(row.name)}</p>
            {str(row.work_report) && <p className="fs-sign-report">{str(row.work_report)}</p>}
            <Input
              label={t("Name of the person signing")}
              value={name}
              autoFocus
              autoComplete="off"
              onChange={(e) => setName(e.target.value)}
            />
            <p className="hint">
              {t("By signing, the customer confirms that the work was done on {date}.", {
                date: formatDate(isoDate(new Date())),
              })}
            </p>
          </form>
        </Dialog>
      )}
    </>
  );
}

export function PrintJob({ busy }: RecordExtProps) {
  return (
    <Button icon="printer" disabled={busy} onClick={() => window.print()}>
      {t("Print report")}
    </Button>
  );
}

/** The parts of a job, reloaded after every write. */
function useParts(job: string): Row[] {
  const d = useData();
  const [rows, setRows] = useState<Row[]>([]);
  const can = d.models.has(PART);
  useEffect(() => {
    if (!can) return;
    let live = true;
    void call(d.token, `${PART}.search`, {
      filter: [{ field: "job_id", op: "eq", value: job }],
      limit: 500,
    }).then((r) => {
      if (live) setRows(r.kind === "ok" ? (r.value as { rows: Row[] }).rows : []);
    });
    return () => {
      live = false;
    };
  }, [d.token, job, can, d.mutations]);
  return rows;
}

/** The job report as it prints: hidden on screen, the only thing on paper. */
export function JobReport({ row }: RecordExtProps) {
  const d = useData();
  const parts = useParts(row.id);
  const model = d.models.get(JOB)?.model;
  const stateField = model?.fields.find((f) => f.name === "state");
  const customer = str(row.customer_id) ? d.refText("party", str(row.customer_id)) : "";
  const tech = str(row.technician_id) ? d.refText("hr_employee", str(row.technician_id)) : "";
  const total = parts.reduce((s, p) => s + num(p.quantity) * num(p.unit_price), 0);
  const start = clockText(row.scheduled_start_minute);
  const length = shownNumber("duration_minutes", row.planned_minutes);
  return (
    <PrintSheet>
      <article className="so-print fs-print">
        <header>
          <h1>{t("Job report")}</h1>
          <p className="so-print-no">{str(row.name)}</p>
        </header>
        <dl className="so-print-meta">
          {customer && (
            <div>
              <dt>{t("Customer")}</dt>
              <dd>{customer}</dd>
            </div>
          )}
          {siteText(row) && (
            <div>
              <dt>{t("Site")}</dt>
              <dd>{[str(row.site_name), siteText(row)].filter(Boolean).join(", ")}</dd>
            </div>
          )}
          <div>
            <dt>{t("Date")}</dt>
            <dd>
              {formatDate(str(row.scheduled_on))}
              {start && ` ${start}`}
              {length && ` (${length})`}
            </dd>
          </div>
          {tech && (
            <div>
              <dt>{t("Technician")}</dt>
              <dd>{tech}</dd>
            </div>
          )}
          <div>
            <dt>{t("Status")}</dt>
            <dd>{optionLabel(str(row.state), stateField)}</dd>
          </div>
        </dl>
        {str(row.description) && (
          <section>
            <h2>{t("Task")}</h2>
            <p className="fs-print-text">{str(row.description)}</p>
          </section>
        )}
        <section>
          <h2>{t("Work done")}</h2>
          <p className="fs-print-text">{str(row.work_report) || "—"}</p>
        </section>
        {parts.length > 0 && (
          <table>
            <thead>
              <tr>
                <th>{t("Part")}</th>
                <th className="num">{t("Quantity")}</th>
                <th className="num">{t("Unit price")}</th>
                <th className="num">{t("Amount")}</th>
              </tr>
            </thead>
            <tbody>
              {parts.map((p) => (
                <tr key={p.id}>
                  <td>
                    {d.refText("product", str(p.product_id))}
                    {str(p.note) && ` (${str(p.note)})`}
                  </td>
                  <td className="num">{num(p.quantity)}</td>
                  <td className="num">{money(num(p.unit_price))}</td>
                  <td className="num">{money(num(p.quantity) * num(p.unit_price))}</td>
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr>
                <th colSpan={3}>{t("Total")}</th>
                <td className="num">{money(total)}</td>
              </tr>
            </tfoot>
          </table>
        )}
        <section className="fs-print-sign">
          <h2>{t("Customer sign-off")}</h2>
          {str(row.signed_by) ? (
            <p>
              {t("Signed by {name} on {date}", {
                name: str(row.signed_by),
                date: formatDate(str(row.signed_on)),
              })}
            </p>
          ) : (
            <p className="fs-print-line">{t("Name and signature")}</p>
          )}
        </section>
      </article>
    </PrintSheet>
  );
}
