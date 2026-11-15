// Inventory: its home has the warehouse jobs (receive goods, move stock, scrap) as buttons, and a
// stock item has "Count stock", which books the difference as a stock count move.
import { lazy } from "react";
import { registerAppPages, registerRecordExtension } from "../extensions.ts";
import { appHome } from "../nav.ts";
import { OVERVIEW, isOverview } from "../route.ts";
import { CountStockAction } from "../apps/inventory/CountStock.tsx";

const InventoryHome = lazy(() =>
  import("../apps/inventory/InventoryHome.tsx").then((m) => ({ default: m.InventoryHome })),
);

registerAppPages("inventory", {
  use: (ctx) => {
    if (!ctx) return {};
    // The overview, and the app's own address when the overview is where it opens (the launcher).
    const home =
      isOverview(ctx.route) || (!ctx.route.model && appHome(ctx.app.module) === OVERVIEW);
    if (!home) return {};
    return { page: <InventoryHome app={ctx.app} route={ctx.route} go={ctx.go} />, overview: true };
  },
});

registerRecordExtension("inventory_item", { actions: [CountStockAction] });
