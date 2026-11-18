// Accounting opens on its journal cards (apps/accounting/Overview.tsx), and its bank transactions
// have a reconcile screen (apps/accounting/Reconcile.tsx) next to the generic list and an "Undo
// booking" button (apps/accounting/Unbook.tsx). "Invoice booking" in the sidebar holds the company's
// booking settings of customer invoices (apps/accounting/BookingSettings.tsx).
import { lazy } from "react";
import { Button, t } from "@obop/ui";
import { BANK_LINE, RECONCILE, reconcileHref } from "../apps/accounting/links.ts";
import { UnbookButton } from "../apps/accounting/Unbook.tsx";
import {
  registerAppPages,
  registerModelPage,
  registerRecordExtension,
  type ModelPageProps,
} from "../extensions.ts";
import { ModelPage } from "../ModelPage.tsx";
import { appHome } from "../nav.ts";
import { OVERVIEW, href, isOverview } from "../route.ts";

const AccountingOverview = lazy(() =>
  import("../apps/accounting/Overview.tsx").then((m) => ({ default: m.AccountingOverview })),
);
const BookingSettings = lazy(() =>
  import("../apps/accounting/BookingSettings.tsx").then((m) => ({ default: m.BookingSettings })),
);
const ReconcileView = lazy(() =>
  import("../apps/accounting/Reconcile.tsx").then((m) => ({ default: m.ReconcileView })),
);

/** The journal model is what the cards are drawn from; without it the generic overview stays. */
const hasJournals = (models: readonly { name: string }[]) =>
  models.some((m) => m.name === "accounting_journal") &&
  models.some((m) => m.name === "accounting_move");

/** The route name of the invoice booking settings inside the Accounting app. */
const BOOKING = "invoice-booking";

registerAppPages("accounting", {
  use: (ctx) => {
    if (!ctx || !hasJournals(ctx.app.module.models)) return {};
    const onBooking = ctx.route.model === BOOKING;
    const navAfter = [
      {
        key: BOOKING,
        label: t("Invoice booking"),
        href: href({ app: ctx.app.id, model: BOOKING }),
        title: t("How a posted customer invoice is booked to the ledger."),
        current: onBooking,
      },
    ];
    if (onBooking) return { navAfter, page: <BookingSettings /> };
    const atHome = !ctx.route.model;
    const home = isOverview(ctx.route) || (atHome && appHome(ctx.app.module) === OVERVIEW);
    return home
      ? { navAfter, page: <AccountingOverview app={ctx.app} />, overview: true }
      : { navAfter };
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
registerRecordExtension(BANK_LINE, { actions: [UnbookButton] });
