// The company's booking settings of customer invoices (owner ruling of 2026-11-17): whether a posted
// invoice books its journal entry right away, and the journal, receivable account, revenue account,
// tax and VAT account it is booked with. Read with `invoice_posting_setting.get`, saved with
// `invoice_posting_setting.update` (accounting.manager); the server checks every record it names.
import { useEffect, useMemo, useState } from "react";
import { Button, Card, Notice, Select, Skeleton, Switch, t, useWorded } from "@obop/ui";
import { call, search, type Row } from "../../api.ts";
import { useData } from "../../data.tsx";
import { serverMessage } from "../../serverMessage.ts";
import "./accounting.css";

const KEYS = ["journal", "receivable_account", "revenue_account", "tax", "vat_account"] as const;
type Key = (typeof KEYS)[number];
type Settings = Record<Key, string> & { book_on_post: boolean; ready: boolean; missing: string[] };

const label = (r: Row) =>
  [r.code, r.name].filter((x) => typeof x === "string" && x).join(" · ") || r.id;

export function BookingSettings() {
  const d = useData();
  const [saved, setSaved] = useState<Settings | null>(null);
  const [form, setForm] = useState<Settings | null>(null);
  const [error, setError] = useWorded();
  const [journals, setJournals] = useState<Row[]>([]);
  const [accounts, setAccounts] = useState<Row[]>([]);
  const [taxes, setTaxes] = useState<Row[]>([]);
  const [busy, setBusy] = useState(false);
  // accounting.manager writes the chart of accounts and these settings alike.
  const canEdit = useMemo(() => !!d.models.get("accounting_account")?.model.can_create, [d.models]);

  useEffect(() => {
    let live = true;
    void (async () => {
      const [s, j, a, x] = await Promise.all([
        call(d.token, "invoice_posting_setting.get", {}),
        search(d.token, "accounting_journal", { limit: 500 }),
        search(d.token, "accounting_account", {
          filter: [{ field: "deprecated", op: "ne", value: true }],
          order: [{ field: "code", dir: "asc" }],
          limit: 2000,
        }),
        search(d.token, "accounting_tax", { limit: 500 }),
      ]);
      if (!live) return;
      if (s.kind !== "ok")
        return setError(() => (s.kind === "error" ? serverMessage(s.message) : ""));
      const v = s.value as Settings;
      setSaved(v);
      setForm(v);
      setJournals("error" in j ? [] : j.rows.filter((r) => r.active !== false));
      setAccounts("error" in a ? [] : a.rows);
      setTaxes("error" in x ? [] : x.rows.filter((r) => r.active !== false));
    })();
    return () => {
      live = false;
    };
  }, [d.token]);

  const set = (k: Key | "book_on_post", v: string | boolean) =>
    setForm((f) => (f ? ({ ...f, [k]: v } as Settings) : f));
  const dirty = !!form && !!saved && KEYS.concat().some((k) => form[k] !== saved[k]);
  const changed = dirty || (!!form && !!saved && form.book_on_post !== saved.book_on_post);

  const save = async () => {
    if (!form) return;
    setBusy(true);
    const body: Record<string, unknown> = { book_on_post: form.book_on_post };
    for (const k of KEYS) body[k] = form[k] ?? "";
    const r = await call(d.token, "invoice_posting_setting.update", body);
    setBusy(false);
    if (r.kind !== "ok")
      return d.notify(r.kind === "error" ? serverMessage(r.message) : t("Not saved"), "error");
    const v = r.value as Settings;
    setSaved(v);
    setForm(v);
    d.notify(t("Booking settings saved"), "ok");
  };

  const pick = (k: Key, title: string, rows: Row[], hint?: string, optional = false) => (
    <Select
      label={title}
      hint={hint}
      value={form?.[k] ?? ""}
      disabled={!canEdit || busy}
      onChange={(e) => set(k, e.target.value)}
    >
      <option value="">{optional ? t("The tax's own account") : t("Not chosen")}</option>
      {rows.map((r) => (
        <option key={r.id} value={r.id}>
          {label(r)}
        </option>
      ))}
    </Select>
  );

  return (
    <section className="main panel acc-settings" aria-label={t("Invoice booking")}>
      <header className="home-head">
        <div>
          <h1>{t("Invoice booking")}</h1>
          <p className="hint">
            {t(
              "How a posted customer invoice is booked to the ledger. The entry is a draft until an approver posts it.",
            )}
          </p>
        </div>
      </header>
      <div className="home-body">
        {error ? (
          <Notice tone="danger">{error}</Notice>
        ) : !form ? (
          <Skeleton rows={5} cols={1} />
        ) : (
          <Card className="acc-settings-card">
            <Switch
              label={t("Book invoices when they are posted")}
              checked={form.book_on_post}
              disabled={!canEdit || busy}
              onChange={(e) => set("book_on_post", e.target.checked)}
            />
            {saved?.book_on_post && !saved.ready && (
              <Notice tone="warning">
                {t(
                  "Invoices are not booked yet: choose a journal, a receivable account, a revenue account and a tax.",
                )}
              </Notice>
            )}
            {pick(
              "journal",
              t("Journal"),
              journals.filter((j) => j.kind === "sale" || j.kind === "general"),
            )}
            {pick(
              "receivable_account",
              t("Receivable account"),
              accounts.filter((a) => a.account_type === "asset_receivable"),
              t(
                "Debited with the invoice total plus VAT, or the total itself when the tax includes VAT in the price.",
              ),
            )}
            {pick(
              "revenue_account",
              t("Revenue account"),
              accounts.filter((a) => String(a.account_type ?? "").startsWith("income")),
              t("Credited with the net amount of each line."),
            )}
            {pick(
              "tax",
              t("Tax"),
              taxes.filter((x) => x.scope === "sales"),
              t("Its rate gives the VAT of the invoice."),
            )}
            {pick(
              "vat_account",
              t("VAT account"),
              accounts.filter((a) => String(a.account_type ?? "").startsWith("liability")),
              undefined,
              true,
            )}
            {canEdit ? (
              <div className="acc-settings-actions">
                <Button
                  variant="primary"
                  loading={busy}
                  disabled={!changed}
                  onClick={() => void save()}
                >
                  {t("Save")}
                </Button>
              </div>
            ) : (
              <p className="hint">{t("An accounting manager can change these settings.")}</p>
            )}
          </Card>
        )}
      </div>
    </section>
  );
}
