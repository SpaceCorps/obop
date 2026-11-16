// Fleet: its home has the reminders a fleet manager checks (licences that expire, planned
// services) next to where the vehicles are (UI plan 3.6).
import { lazy } from "react";
import { registerAppPages } from "../extensions.ts";
import { appHome } from "../nav.ts";
import { OVERVIEW, isOverview } from "../route.ts";

const FleetHome = lazy(() =>
  import("../apps/fleet/FleetHome.tsx").then((m) => ({ default: m.FleetHome })),
);

registerAppPages("fleet", {
  use: (ctx) => {
    if (!ctx || !ctx.app.module.models.some((m) => m.name === "fleet_vehicle_service")) return {};
    // The overview, and the app's own address when the overview is where it opens.
    const home =
      isOverview(ctx.route) || (!ctx.route.model && appHome(ctx.app.module) === OVERVIEW);
    return home ? { page: <FleetHome app={ctx.app} />, overview: true } : {};
  },
});
