// "Bill time" on a project (K62): the approved, billable time of a period becomes a draft invoice
// for the project's customer at its hourly rate. The server does all of it in one typed command,
// `time_billing.create_invoice` (`obop call time_billing create_invoice project=WEB from=... to=...`
// on the command line, `POST /api/time_billing/create_invoice` over REST, the MCP tool
// `time_billing__create_invoice`): "Preview" is its dry run, "Create draft invoice" the real one.
// The customer and the rate are the project's unless changed here; the invoice stays a draft.
import { useState } from "react";
import { fmt, getCurrency, t } from "@obop/ui";
import type { Reply } from "../../api.ts";
import { useData } from "../../data.tsx";
import type { Edit } from "../../edit.ts";
import type { RecordExtProps } from "../../extensions.ts";
import { RefPicker } from "../../fields.tsx";
import { serverMessage } from "../../serverMessage.ts";
import { Button, Dialog, Input, Segmented } from "../../ui.tsx";
import "../supply.css";
import "./projects.css";

const INVOICE = "invoice";

type Planned = {
  entries?: number;
  hours?: string;
  net?: number;
  invoice_id?: string;
  lines?: { description: string; amount: number }[];
};

/** The first and the last day of the month before `today` (YYYY-MM-DD). */
export function lastMonth(today: Date): { from: string; to: string } {
  const first = new Date(Date.UTC(today.getFullYear(), today.getMonth() - 1, 1));
  const last = new Date(Date.UTC(today.getFullYear(), today.getMonth(), 0));
  const day = (d: Date) => d.toISOString().slice(0, 10);
  return { from: day(first), to: day(last) };
}

export function BillTimeAction({ row, fields, busy }: RecordExtProps) {
  const d = useData();
  const [open, setOpen] = useState(false);
  // Only where there is time to bill (Timesheets) and the person may make invoices; the server
  // says in words when a group is missing (timesheets.manager).
  if (!d.models.get(INVOICE)?.model.can_create || !d.models.get("timesheets_entry")) return null;
  return (
    <>
      <Button icon="invoicing" disabled={busy} onClick={() => setOpen(true)}>
        {t("Bill time")}
      </Button>
      {open && <BillDialog row={row} fields={fields} onClose={() => setOpen(false)} />}
    </>
  );
}

function BillDialog({
  row,
  fields,
  onClose,
}: {
  row: RecordExtProps["row"];
  fields: RecordExtProps["fields"];
  onClose: () => void;
}) {
  const d = useData();
  const cur = getCurrency();
  const customerField = fields.find((f) => f.name === "customer_id");
  const period = lastMonth(new Date());
  const [customer, setCustomer] = useState<Edit>(String(row.customer_id ?? ""));
  const rate0 = typeof row.hourly_rate === "number" ? fmt.moneyInput(row.hourly_rate, cur) : "";
  const [rate, setRate] = useState(rate0);
  const [from, setFrom] = useState(period.from);
  const [to, setTo] = useState(period.to);
  const [per, setPer] = useState<"entry" | "task">("task");
  const [plan, setPlan] = useState<Planned | null>(null);
  const [error, setError] = useState("");
  const [working, setWorking] = useState(false);

  const minor = rate.trim() ? fmt.parseMoney(rate, cur)?.minor : undefined;
  const body = (dry: boolean) => {
    const b: Record<string, unknown> = { project: row.id, from, to, per, dry_run: dry };
    if (customer) b.customer = String(customer);
    if (minor !== undefined) b.hourly_rate_minor = minor;
    return b;
  };
  const failure = (r: Reply) =>
    r.kind === "error" ? serverMessage(r.message) : t("Sent for approval");

  async function go(dry: boolean) {
    // A project without a customer or a rate has them asked here; the server would answer with
    // its own field names (customer, hourly_rate_minor).
    if (!String(customer ?? "").trim()) {
      setError(t("Choose the customer to bill"));
      return;
    }
    if (!rate.trim() || minor === undefined) {
      setError(t("Enter the hourly rate as an amount, such as 95.00"));
      return;
    }
    setWorking(true);
    setError("");
    const r = await d.run("time_billing.create_invoice", body(dry));
    setWorking(false);
    if (r.kind !== "ok") {
      setPlan(null);
      setError(failure(r));
      return;
    }
    const p = r.value as Planned;
    if (dry) {
      setPlan(p);
      return;
    }
    await d.changed(INVOICE);
    d.notify(t("Draft invoice created"), "ok");
    onClose();
    const link = p.invoice_id ? d.linkTo(INVOICE, { id: p.invoice_id }) : null;
    if (link) location.hash = link;
  }

  // A change of what is asked makes the preview stale.
  const edit =
    <T,>(set: (v: T) => void) =>
    (v: T) => {
      set(v);
      setPlan(null);
    };

  return (
    <Dialog
      open
      size="md"
      title={t("Bill time")}
      onClose={() => !working && onClose()}
      className="sup-dialog"
      footer={
        <>
          <Button onClick={onClose} disabled={working}>
            {t("Cancel")}
          </Button>
          {plan ? (
            <Button variant="primary" loading={working} onClick={() => void go(false)}>
              {t("Create draft invoice")}
            </Button>
          ) : (
            <Button variant="primary" loading={working} onClick={() => void go(true)}>
              {t("Preview")}
            </Button>
          )}
        </>
      }
    >
      <form
        className="sup-form"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          void go(!plan);
        }}
      >
        {customerField && (
          <div className="field">
            <label htmlFor="bill-customer">{t("Customer")}</label>
            <RefPicker
              id="bill-customer"
              field={customerField}
              value={customer}
              onChange={edit(setCustomer)}
            />
          </div>
        )}
        <Input
          id="bill-rate"
          label={t("Hourly rate ({currency})", { currency: cur })}
          inputMode="decimal"
          value={rate}
          onChange={(e) => edit(setRate)(e.currentTarget.value)}
        />
        <div className="prj-bill-period">
          <Input
            id="bill-from"
            type="date"
            label={t("From")}
            value={from}
            onChange={(e) => edit(setFrom)(e.currentTarget.value)}
          />
          <Input
            id="bill-to"
            type="date"
            label={t("To")}
            value={to}
            onChange={(e) => edit(setTo)(e.currentTarget.value)}
          />
        </div>
        <Segmented
          label={t("Invoice lines")}
          value={per}
          onChange={edit(setPer)}
          options={[
            { value: "task", label: t("One per task") },
            { value: "entry", label: t("One per time entry") },
          ]}
        />
        {plan && (
          <div className="prj-bill-preview" aria-live="polite">
            <ul>
              {(plan.lines ?? []).map((l, i) => (
                <li key={i}>
                  <span>{l.description}</span>
                  <span className="num">{fmt.money(l.amount, cur)}</span>
                </li>
              ))}
            </ul>
            <p className="prj-bill-total">
              <span>
                {t("{count, plural, one {# entry} other {# entries}} · {hours} h", {
                  count: plan.entries ?? 0,
                  hours: plan.hours ?? "0",
                })}
              </span>
              <strong className="num">{fmt.money(plan.net ?? 0, cur)}</strong>
            </p>
          </div>
        )}
        <p className="hint">
          {t(
            "Approved, billable time of the period that is not billed yet becomes a draft invoice. Posting it stays a separate step.",
          )}
        </p>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
      </form>
    </Dialog>
  );
}
