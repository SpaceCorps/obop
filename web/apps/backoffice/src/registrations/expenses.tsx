// Expenses: "Settings" in the sidebar holds the company's expense settings, such as whether the
// person who made a claim may approve it (apps/expenses/ExpenseSettings.tsx).
import { lazy } from "react";
import { t } from "@obop/ui";
import { registerAppPages } from "../extensions.ts";
import { href } from "../route.ts";

const ExpenseSettings = lazy(() =>
  import("../apps/expenses/ExpenseSettings.tsx").then((m) => ({ default: m.ExpenseSettings })),
);

/** The route name of the settings page inside the Expenses app. */
const SETTINGS = "settings";

registerAppPages("expenses", {
  use: (ctx) => {
    if (!ctx) return {};
    const on = ctx.route.model === SETTINGS;
    return {
      navAfter: [
        {
          key: SETTINGS,
          label: t("Settings"),
          href: href({ app: ctx.app.id, model: SETTINGS }),
          title: t("Rules for the expense claims of the whole company."),
          current: on,
        },
      ],
      ...(on ? { page: <ExpenseSettings /> } : {}),
    };
  },
});
