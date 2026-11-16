// The Connectors app draws its own sidebar and pages (D54): the catalogue, the providers and the
// connections are not a plain list of records.
import { ConnectorsApp, ConnectorsNav } from "../connectors/ConnectorsApp.tsx";
import { registerAppPages } from "../extensions.ts";

registerAppPages("connectors", {
  use: (ctx) =>
    ctx
      ? {
          nav: <ConnectorsNav app={ctx.app} route={ctx.route} />,
          page: <ConnectorsApp app={ctx.app} route={ctx.route} go={ctx.go} />,
        }
      : {},
});
