// The Dashboards app opens on its overview, the tiles drawn from the dashboards, not on a list of
// dashboard records.
import { lazy } from "react";
import { t } from "@obop/ui";
import { BOARD, boardHref } from "../dashboards/route.ts";
import { registerAppPages } from "../extensions.ts";
import type { App } from "../Chrome.tsx";

const DashboardsApp = lazy(() =>
  import("../dashboards/DashboardsApp.tsx").then((m) => ({ default: m.DashboardsApp })),
);

const hasBoards = (app: App) => app.module.models.some((m) => m.name === "dashboards_dashboard");

registerAppPages("dashboards", {
  home: (app) => (hasBoards(app) ? boardHref(app.id) : undefined),
  use: (ctx) => {
    if (!ctx || !hasBoards(ctx.app)) return {};
    const { app, route, go, email } = ctx;
    const onBoard = route.model === "" || route.model === BOARD;
    return {
      navBefore: [
        {
          key: "board",
          label: t("Overview"),
          href: boardHref(app.id),
          title: t("Tiles drawn from your dashboards"),
          current: onBoard,
        },
      ],
      page: onBoard ? <DashboardsApp app={app} route={route} go={go} owner={email} /> : null,
    };
  },
});
