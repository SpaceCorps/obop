// A Swedish VAT report shown as the return it prepares (momsdeklaration): box 49 at the top, then
// the boxes in the sections of the form with the labels of your box configuration, the amounts in
// the company's currency, and "Compute from ledger" while the report is a draft. It reads with
// `accounting_se_vat_box.search`, `accounting_se_vat_report_line.search` and
// `accounting_se_company.search`, and computes with `vat_computation.compute` (an agent may only
// dry-run it). UNVERIFIED, as the package is: the page says so.
import { useEffect, useState } from "react";
import { Button, Icon, Notice, fmt, t } from "@obop/ui";
import { call, search, type Row } from "../../api.ts";
import { useData } from "../../data.tsx";
import type { RecordExtProps } from "../../extensions.ts";
import { href } from "../../route.ts";
import { serverMessage } from "../../serverMessage.ts";
import { returnSections, type SectionKey } from "./boxes.ts";
import "./accounting_se.css";

const BOX = "accounting_se_vat_box";
const LINE = "accounting_se_vat_report_line";
const REPORT = "accounting_se_vat_report";
const CODE = "accounting_se_vat_code";

type Data = {
  boxes: Row[];
  lines: Row[];
  currency: string;
  untaxed: string[];
  /** code -> record id, so each code in the notice opens where its tax is chosen. */
  codeIds: Record<string, string>;
};

/** The VAT codes that feed a box but name no tax: no ledger line can carry them (the compute refuses). */
export function untaxedCodes(codes: readonly Row[]): string[] {
  return codes
    .filter((c) => (!!c.base_box_id || !!c.vat_box_id) && !c.tax_id)
    .map((c) => String(c.code ?? ""))
    .filter(Boolean)
    .sort();
}

function sectionTitle(k: SectionKey): string {
  switch (k) {
    case "A":
      return t("A. Sales and withdrawals subject to VAT, without VAT");
    case "B":
      return t("B. Output VAT on sales and withdrawals");
    case "C":
      return t("C. Purchases subject to VAT where the buyer pays the VAT");
    case "D":
      return t("D. Output VAT on those purchases");
    case "H":
      return t("H. Imports");
    case "I":
      return t("I. Output VAT on imports");
    case "E":
      return t("E. Sales exempt from VAT, and sales abroad");
    case "F":
      return t("F. Input VAT");
    case "G":
      return t("G. VAT to pay or to get back");
    default:
      return t("Other boxes");
  }
}

/** One reason of a refused compute, in plain words where it is the common one. */
function problemText(x: string): string {
  const m = /^code (.+): maps to a box but has no tax_id/.exec(x);
  return m ? t("VAT code {code} feeds a box but has no tax.", { code: m[1]! }) : x;
}

export function VatReturnPanel({ row }: RecordExtProps) {
  const d = useData();
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);
  /** The server's reasons why the last compute was refused, one per line. */
  const [problems, setProblems] = useState<string[]>([]);

  useEffect(() => {
    let live = true;
    void (async () => {
      const [boxes, lines, company, codes] = await Promise.all([
        search(d.token, BOX, { limit: 500 }),
        search(d.token, LINE, {
          filter: [{ field: "report_id", op: "eq", value: row.id }],
          limit: 1000,
        }),
        typeof row.company_id === "string" && row.company_id
          ? search(d.token, "accounting_se_company", {
              filter: [{ field: "id", op: "eq", value: row.company_id }],
            })
          : Promise.resolve(null),
        search(d.token, CODE, { limit: 500 }),
      ]);
      if (!live) return;
      if ("error" in boxes) return setError(boxes.error);
      if ("error" in lines) return setError(lines.error);
      const cur = company && !("error" in company) ? company.rows[0]?.currency : undefined;
      setError("");
      setData({
        boxes: boxes.rows,
        lines: lines.rows,
        currency: typeof cur === "string" && /^[A-Z]{3}$/.test(cur) ? cur : "SEK",
        untaxed: "error" in codes ? [] : untaxedCodes(codes.rows),
        codeIds:
          "error" in codes
            ? {}
            : Object.fromEntries(codes.rows.map((c) => [String(c.code ?? ""), c.id])),
      });
    })();
    return () => {
      live = false;
    };
  }, [d.token, row.id, row.company_id, reload, d.mutations]);

  const draft = row.state === "draft";
  const toPay = typeof row.vat_to_pay === "number" ? row.vat_to_pay : 0;
  // One sign convention on the whole screen, the form's: box 49 below zero is VAT to get back.
  const money = (minor: number) => fmt.money(minor, data?.currency ?? "SEK");
  const codesHref = href({ app: "accounting_se", model: CODE });

  async function compute() {
    setBusy(true);
    const r = await call(d.token, "vat_computation.compute", { report: row.id });
    setBusy(false);
    setProblems([]);
    if (r.kind === "error" && r.message.startsWith("not computed: ")) {
      // The configuration is not complete: say what, beside the button, not in a passing toast.
      setProblems(
        r.message
          .replace(/^not computed: /, "")
          .split("; ")
          .filter(Boolean),
      );
      return;
    }
    if (r.kind === "ok") {
      const v = r.value as { moves?: number; lines?: unknown[] };
      d.notify(
        t("Computed from {moves} posted entries: {boxes} boxes filled.", {
          moves: fmt.integer(v.moves ?? 0),
          boxes: fmt.integer(v.lines?.length ?? 0),
        }),
      );
      await Promise.all([d.changed(REPORT), d.changed(LINE)]);
      setReload((n) => n + 1);
    } else if (r.kind === "approval")
      d.notify(t("Approval needed: nothing changes until a human approves it."), "info");
    else d.notify(serverMessage(r.message), "error");
  }

  const sections = data ? returnSections(data.boxes, data.lines, toPay) : [];
  const unverified = data ? data.boxes.filter((b) => b.review !== "verified").length : 0;

  return (
    <section className="vat-return" aria-labelledby={`vat-ret-${row.id}`}>
      <Notice tone="warning" title={t("Experimental: not verified against SKV 4700")}>
        {t(
          "Compare every box with the current form before you file. Nothing is sent to Skatteverket from here.",
        )}
      </Notice>

      <div className={`vat-49${toPay < 0 ? " refund" : ""}`}>
        <div className="vat-49-text">
          <span className="vat-box-no" aria-hidden="true">
            49
          </span>
          <div>
            <h2 id={`vat-ret-${row.id}`}>
              {toPay > 0 ? t("VAT to pay") : toPay < 0 ? t("VAT to get back") : t("No VAT to pay")}
            </h2>
            <p className="hint">
              {t("Box 49 of the return: output VAT minus input VAT; below zero you get it back.")}
            </p>
          </div>
        </div>
        <strong className="vat-49-amount tabular">{money(toPay)}</strong>
      </div>

      {draft && data && data.untaxed.length > 0 && problems.length === 0 && (
        <Notice tone="warning" title={t("Some VAT codes cannot be computed yet")}>
          <p>
            {t(
              "These VAT codes feed a box but have no tax, so no ledger line can carry them: {codes}. Give each one its tax, or remove its boxes.",
              { codes: data.untaxed.join(", ") },
            )}
          </p>
          <ul className="vat-untaxed" aria-label={t("Codes without a tax")}>
            {data.untaxed.map((code) => (
              <li key={code}>
                <a
                  className="btn small"
                  href={
                    data.codeIds[code]
                      ? href({
                          app: "accounting_se",
                          model: CODE,
                          id: data.codeIds[code],
                          field: "tax_id",
                        })
                      : codesHref
                  }
                >
                  {t("Choose the tax of {code}", { code })}
                </a>
              </li>
            ))}
          </ul>
          <a className="btn vat-fix" href={codesHref}>
            {t("Open VAT codes")}
          </a>
        </Notice>
      )}
      {draft && problems.length > 0 && (
        <Notice tone="danger" title={t("Not computed: the VAT configuration is not complete")}>
          <ul className="vat-problems">
            {problems.map((x) => (
              <li key={x}>{problemText(x)}</li>
            ))}
          </ul>
          <a className="btn vat-fix" href={codesHref}>
            {t("Open VAT codes")}
          </a>
        </Notice>
      )}
      {draft && (
        <div className="vat-compute">
          <Button
            icon="replay"
            loading={busy}
            // A code without a tax is refused by the server: say so above instead of asking it.
            disabled={!data || data.untaxed.length > 0}
            onClick={() => void compute()}
          >
            {t("Compute from ledger")}
          </Button>
          <p className="hint">
            {t(
              "Sums the posted entries of the period by your VAT codes and replaces the boxes of this draft.",
            )}
          </p>
        </div>
      )}

      {error ? (
        <p className="hint">{error}</p>
      ) : !data ? (
        <div className="vat-wait" aria-busy="true" aria-label={t("Loading")}>
          <span className="skel" />
          <span className="skel short" />
        </div>
      ) : sections.length === 0 ? (
        <p className="hint">
          {t("No VAT box is set up yet. Add the boxes under Configuration, VAT boxes.")}
        </p>
      ) : (
        <div className="vat-sections">
          {sections.map((s) => (
            <section key={s.key} className="vat-section" aria-label={sectionTitle(s.key)}>
              <h3 className="overline">{sectionTitle(s.key)}</h3>
              <ul>
                {s.rows.map((b) => (
                  <li key={b.id} className={b.box === "49" ? "vat-row total" : "vat-row"}>
                    <span className="vat-box-no">{b.box}</span>
                    <span className="vat-label">
                      {b.label}
                      {!b.verified && (
                        <span className="vat-unverified" title={t("This box is not verified yet")}>
                          <Icon name="warning" size={12} />
                          {t("Unverified")}
                        </span>
                      )}
                    </span>
                    <span
                      className={`vat-amount tabular${b.filled ? "" : " vat-none"}`}
                      title={b.vouchers || undefined}
                    >
                      {b.filled ? money(b.amount) : "—"}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ))}
          {unverified > 0 && (
            <p className="hint vat-foot">
              {t("{count} boxes of your configuration are not verified yet.", {
                count: fmt.integer(unverified),
              })}
            </p>
          )}
        </div>
      )}
    </section>
  );
}
