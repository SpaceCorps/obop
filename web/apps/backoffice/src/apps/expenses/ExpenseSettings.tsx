// The company's expense settings (owner ruling of 2026-11-17): whether the person who made a claim
// may approve it too. Off by default: the server refuses such an approval. Read with
// `expenses_setting.get`, changed with `expenses_setting.update` (administrators only).
import { useEffect, useState } from "react";
import { Card, Notice, Skeleton, Switch, t } from "@obop/ui";
import { call } from "../../api.ts";
import { useData } from "../../data.tsx";
import { serverMessage } from "../../serverMessage.ts";
import "./expenses.css";

export function ExpenseSettings() {
  const d = useData();
  const [allow, setAllow] = useState<boolean | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  // Administrators see Customize; they are the ones who may change a company setting.
  const isAdmin = d.apps.some((a) => a.id === "customize");

  useEffect(() => {
    let live = true;
    void call(d.token, "expenses_setting.get", {}).then((r) => {
      if (!live) return;
      if (r.kind === "ok")
        setAllow(!!(r.value as { allow_self_approval?: boolean }).allow_self_approval);
      else setError(r.kind === "error" ? serverMessage(r.message) : "");
    });
    return () => {
      live = false;
    };
  }, [d.token]);

  const change = async (v: boolean) => {
    // The switch moves at once; it goes back when the server does not take the change.
    const was = allow;
    setAllow(v);
    setBusy(true);
    const r = await call(d.token, "expenses_setting.update", { allow_self_approval: v });
    setBusy(false);
    if (r.kind !== "ok") {
      setAllow(was);
      return d.notify(r.kind === "error" ? serverMessage(r.message) : t("Not saved"), "error");
    }
    setAllow(!!(r.value as { allow_self_approval?: boolean }).allow_self_approval);
    d.notify(t("Expense settings saved"), "ok");
  };

  return (
    <section className="main panel exp-settings" aria-label={t("Expense settings")}>
      <header className="home-head">
        <div>
          <h1>{t("Expense settings")}</h1>
          <p className="hint">{t("Rules for the expense claims of the whole company.")}</p>
        </div>
      </header>
      <div className="home-body">
        {error ? (
          <Notice tone="danger">{error}</Notice>
        ) : allow === null ? (
          <Skeleton rows={2} cols={1} />
        ) : (
          <Card className="exp-settings-card">
            <Switch
              label={t("People may approve their own claims")}
              checked={allow}
              disabled={!isAdmin || busy}
              onChange={(e) => void change(e.target.checked)}
            />
            <p className="hint">
              {allow
                ? t("A manager can approve a claim they made themselves.")
                : t("A claim is approved by someone other than the person who made it.")}
            </p>
            {!isAdmin && <p className="hint">{t("An administrator can change this setting.")}</p>}
          </Card>
        )}
      </div>
    </section>
  );
}
