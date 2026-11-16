// Accounting opens on its journal cards (apps/accounting/Overview.tsx), and its bank transactions
// have a reconcile screen (apps/accounting/Reconcile.tsx) next to the generic list.
import { lazy } from "react";
import { Button, t } from "@obop/ui";
import { BANK_LINE, RECONCILE, reconcileHref } from "../apps/accounting/links.ts";
import { registerAppPages, registerModelPage, type ModelPageProps } from "../extensions.ts";
import { ModelPage } from "../ModelPage.tsx";
import { appHome } from "../nav.ts";
import { OVERVIEW, isOverview } from "../route.ts";

const AccountingOverview = lazy(() =>
  import("../apps/accounting/Overview.tsx").then((m) => ({ default: m.AccountingOverview })),
);
const ReconcileView = lazy(() =>
  import("../apps/accounting/Reconcile.tsx").then((m) => ({ default: m.ReconcileView })),
);

/** The journal model is what the cards are drawn from; without it the generic overview stays. */
const hasJournals = (models: readonly { name: string }[]) =>
  models.some((m) => m.name === "accounting_journal") &&
  models.some((m) => m.name === "accounting_move");

registerAppPages("accounting", {
  use: (ctx) => {
    if (!ctx || !hasJournals(ctx.app.module.models)) return {};
    const atHome = !ctx.route.model;
    const home = isOverview(ctx.route) || (atHome && appHome(ctx.app.module) === OVERVIEW);
    return home ? { page: <AccountingOverview app={ctx.app} />, overview: true } : {};
  },
});

/** Bank transactions: the generic list with a Reconcile button, or the reconcile screen. */
function BankTransactionsPage(p: ModelPageProps) {
  if (p.route.view === RECONCILE) return <ReconcileView {...p} />;
  return (
    <ModelPage
      {...p}
      extra={
        <Button icon="check" onClick={() => (location.hash = reconcileHref(p.app.id))}>
          {t("Match transactions")}
        </Button>
      }
    />
  );
}

registerModelPage(BANK_LINE, BankTransactionsPage);
