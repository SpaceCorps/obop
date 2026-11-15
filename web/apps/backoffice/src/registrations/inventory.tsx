// Inventory: its home has the warehouse jobs (receive goods, move stock, scrap) as buttons, and a
// stock item has "Count stock", which books the difference as a stock count move.
import { lazy } from "react";
import { registerAppPages, registerRecordExtension } from "../extensions.ts";
import { isOverview } from "../route.ts";
import { CountStockAction } from "../apps/inventory/CountStock.tsx";

const InventoryHome = lazy(() =>
  import("../apps/inventory/InventoryHome.tsx").then((m) => ({ default: m.InventoryHome })),
);

registerAppPages("inventory", {
  use: (ctx) => {
    if (!ctx || !isOverview(ctx.route)) return {};
    return { page: <InventoryHome app={ctx.app} route={ctx.route} go={ctx.go} /> };
  },
});

registerRecordExtension("inventory_item", { actions: [CountStockAction] });
