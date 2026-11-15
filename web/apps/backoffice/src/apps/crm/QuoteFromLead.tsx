// "New quotation" on a lead (UI plan 3.3, W3 #3): a sale order is created with `lead_id` set, and
// the server fills its customer from the lead (`fill_from = lead_id.party_id`). A lead without a
// customer asks for one first and keeps it (`lead.update`), so the next quotation needs no question.
// Both are plain commands: `obop call sale_order create lead_id=…` on the command line,
// `POST /api/sale_order/create {"lead_id": …}` over REST and the MCP tool `sale_order__create`.
import { useState } from "react";
import { Button, Dialog, t } from "@obop/ui";
import type { Reply } from "../../api.ts";
import { useData } from "../../data.tsx";
import type { Edit } from "../../edit.ts";
import type { RecordExtProps } from "../../extensions.ts";
import { RefPicker } from "../../fields.tsx";
import { serverMessage } from "../../serverMessage.ts";

const ORDER = "sale_order";

const failure = (r: Reply) =>
  r.kind === "error" ? serverMessage(r.message) : t("Sent for approval");

export function QuoteFromLead({ row, fields, busy }: RecordExtProps) {
  const d = useData();
  const order = d.models.get(ORDER)?.model;
  const partyField = fields.find((f) => f.name === "party_id");
  const [asking, setAsking] = useState(false);
  const [party, setParty] = useState<Edit>("");
  const [working, setWorking] = useState(false);
  // Only where Sales can link a quotation to a lead and the person may make one.
  if (!order?.can_create || !order.fields.some((f) => f.name === "lead_id")) return null;
  if (row.stage === "lost") return null;

  const make = async (customer?: string) => {
    setWorking(true);
    try {
      if (customer) {
        const u = await d.run("lead.update", { id: row.id, party_id: customer });
        if (u.kind !== "ok") return d.notify(failure(u), "error");
      }
      const r = await d.run(`${ORDER}.create`, { lead_id: row.id });
      if (r.kind !== "ok" || !r.value || typeof r.value !== "object")
        return d.notify(failure(r), "error");
      const made = r.value as { id: string };
      setAsking(false);
      d.notify(t("Quotation created"), "ok");
      const to = d.linkTo(ORDER, { id: made.id });
      if (to) location.hash = to;
    } finally {
      setWorking(false);
    }
  };

  return (
    <>
      <Button
        icon="sales"
        disabled={busy || working}
        loading={working && !asking}
        onClick={() => (row.party_id || !partyField ? void make() : setAsking(true))}
      >
        {t("New quotation")}
      </Button>
      {asking && partyField && (
        <Dialog
          open
          size="sm"
          onClose={() => !working && setAsking(false)}
          title={t("Who is the customer?")}
          footer={
            <>
              <Button onClick={() => setAsking(false)} disabled={working}>
                {t("Cancel")}
              </Button>
              <Button
                variant="primary"
                loading={working}
                disabled={!party}
                onClick={() => void make(String(party))}
              >
                {t("Create quotation")}
              </Button>
            </>
          }
        >
          <div className="field">
            <label htmlFor="quote-customer">{t("Customer")}</label>
            <RefPicker
              id="quote-customer"
              field={partyField}
              value={party}
              onChange={setParty}
              autoFocus
            />
            <p className="hint">
              {t("The quotation is made out to this contact, and the lead keeps it.")}
            </p>
          </div>
        </Dialog>
      )}
    </>
  );
}
