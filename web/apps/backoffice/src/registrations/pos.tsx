// The Point of Sale opens on its register: the cashier screen with the product grid, the cart and
// the payment (apps/pos/PosTerminal.tsx). Sessions, sales and tills stay in the sidebar.
import { lazy } from "react";
import { t } from "@obop/ui";
import { registerAppPages } from "../extensions.ts";
import { href } from "../route.ts";
import type { App } from "../Chrome.tsx";

const PosRegister = lazy(() =>
  import("../apps/pos/PosTerminal.tsx").then((m) => ({ default: m.PosRegister })),
);

/** The route name of the register inside the Point of Sale app: `#/pos/register`. */
const REGISTER = "register";

const hasRegister = (app: App) =>
  ["pos_session", "pos_order", "pos_order_line", "pos_payment", "pos_config"].every((n) =>
    app.module.models.some((m) => m.name === n),
  );

registerAppPages("pos", {
  home: (app) => (hasRegister(app) ? href({ app: app.id, model: REGISTER }) : undefined),
  use: (ctx) => {
    if (!ctx || !hasRegister(ctx.app)) return {};
    const on = ctx.route.model === "" || ctx.route.model === REGISTER;
    return {
      navBefore: [
        {
          key: REGISTER,
          label: t("Register"),
          href: href({ app: ctx.app.id, model: REGISTER }),
          title: t("Ring up sales at the till"),
          current: on,
        },
      ],
      page: on ? <PosRegister app={ctx.app} go={ctx.go} /> : null,
    };
  },
});
