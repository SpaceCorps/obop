// Employees: an org chart page in the sidebar (who reports to whom), and a new leave request that
// starts on the signed-in person's own employee record (UI plan 3.6).
import { lazy } from "react";
import { t } from "@obop/ui";
import { registerAppPages, registerModelPage } from "../extensions.ts";
import { href } from "../route.ts";
import { LeavePage } from "../apps/hr/LeavePage.tsx";
import { EMPLOYEE, LEAVE, ORG_CHART } from "../apps/hr/links.ts";

const OrgChart = lazy(() =>
  import("../apps/hr/OrgChart.tsx").then((m) => ({ default: m.OrgChart })),
);

registerAppPages("hr", {
  use: (ctx) => {
    if (!ctx || !ctx.app.module.models.some((m) => m.name === EMPLOYEE)) return {};
    const here = ctx.route.model === ORG_CHART;
    return {
      navBefore: [
        {
          key: ORG_CHART,
          label: t("Org chart"),
          href: href({ app: ctx.app.id, model: ORG_CHART }),
          title: t("Who reports to whom"),
          current: here,
        },
      ],
      page: here ? <OrgChart app={ctx.app} /> : null,
    };
  },
});

registerModelPage(LEAVE, LeavePage);
