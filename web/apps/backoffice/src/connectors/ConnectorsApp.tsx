import { useState } from "react";
import { t } from "@obop/ui";
import type { App } from "../Chrome.tsx";
import { useData, useRows } from "../data.tsx";
import { modelTitle } from "../labels.ts";
import { ModelPage } from "../ModelPage.tsx";
import { href, type Go, type Route } from "../route.ts";
import { Button, Icon } from "../ui.tsx";
import { Catalog } from "./Catalog.tsx";
import { ConnectionDetail } from "./ConnectionDetail.tsx";
import type { Provider } from "./model.ts";
import { Compliance, Overview, PreviewPage, Register } from "./Sections.tsx";
import { Wizard } from "./Wizard.tsx";

/** Pages of the app that are not a model's own list. */
const OWN = ["overview", "catalog", "preview", "register", "compliance"] as const;
type Own = (typeof OWN)[number];

const title = (s: Own) =>
  s === "overview"
    ? t("Overview")
    : s === "catalog"
      ? t("Catalog")
      : s === "preview"
        ? t("Privacy preview")
        : s === "register"
          ? t("Processing register")
          : t("Compliance");

/** The app's menu: its own pages first, then the generated lists of its records. */
export function ConnectorsNav({ app, route }: { app: App; route: Route }) {
  const d = useData();
  const has = (m: string) => d.models.has(m);
  const current = route.model || "overview";
  const link = (model: string, label: string) => (
    <a
      key={model}
      href={href({ app: app.id, model })}
      aria-current={current === model ? "page" : undefined}
    >
      {label}
    </a>
  );
  const record = (m: string) => {
    const info = d.models.get(m);
    return info ? link(m, modelTitle(info.model, info.module.id)) : null;
  };
  return (
    <>
      {link("overview", title("overview"))}
      {link("catalog", title("catalog"))}
      {link("connection", t("Connections"))}
      {link("preview", title("preview"))}
      <span className="overline side-label" aria-hidden="true">
        {t("Records")}
      </span>
      {has("data_flow_log") && link("data_flow_log", t("Data flow log"))}
      {has("sync_run") && link("sync_run", t("Sync runs"))}
      {has("consent") && link("consent", t("Legal basis records"))}
      {has("processing_register") && (
        <>
          <span className="overline side-label" aria-hidden="true">
            {t("Administration")}
          </span>
          {link("register", title("register"))}
          {link("compliance", title("compliance"))}
          {record("provider_policy")}
          {record("egress_policy")}
        </>
      )}
    </>
  );
}

function Frame({ app, label, children }: { app: App; label: string; children: React.ReactNode }) {
  return (
    <section className="main panel cx" aria-label={app.name}>
      <h1 className="sr-only">{label}</h1>
      <div className="controlpanel">
        <div className="cp-top">
          <div className="crumbs">
            <span>{app.name}</span>
            <Icon name="chevron-right" size={12} />
            <strong>{label}</strong>
          </div>
        </div>
      </div>
      <div className="content pad">{children}</div>
    </section>
  );
}

/** The Connectors app: the guided flow, the catalogue and the compliance pages around the generated records. */
export function ConnectorsApp({ app, route, go }: { app: App; route: Route; go: Go }) {
  const d = useData();
  const providers = useRows("connector_provider");
  const [wizard, setWizard] = useState<{ initial?: string } | null>(null);
  const [detail, setDetail] = useState<string | null>(null);
  const isAdmin = d.models.has("processing_register");
  const model = app.module.models.find((m) => m.name === route.model);
  // An address that names neither a page nor a model of the app opens the overview.
  const adminOnly = ["register", "compliance"];
  const section =
    ((OWN as readonly string[]).includes(route.model) || model) &&
    !(adminOnly.includes(route.model) && !isAdmin)
      ? route.model
      : "overview";

  const openWizard = (initial?: string) => setWizard({ initial });
  const wizardEl = wizard && (
    <Wizard
      providers={providers.rows as unknown as Provider[]}
      initial={wizard.initial}
      isAdmin={isAdmin}
      onClose={() => setWizard(null)}
      onDone={async (id) => {
        setWizard(null);
        await Promise.all(
          ["connection", "consent", "sync_run", "data_flow_log"].map((m) => d.store.load(m)),
        );
        if (id) setDetail(id);
      }}
    />
  );
  const detailEl = detail && <ConnectionDetail id={detail} onClose={() => setDetail(null)} />;

  if ((OWN as readonly string[]).includes(section)) {
    const s = section as Own;
    return (
      <>
        <Frame app={app} label={title(s)}>
          {s === "overview" && (
            <Overview
              isAdmin={isAdmin}
              onConnect={() => openWizard()}
              onOpen={setDetail}
              goCompliance={() => go({ app: app.id, model: "compliance" })}
            />
          )}
          {s === "catalog" && <Catalog app={app} isAdmin={isAdmin} onConnect={openWizard} />}
          {s === "preview" && <PreviewPage />}
          {s === "register" && isAdmin && <Register />}
          {s === "compliance" && isAdmin && <Compliance />}
        </Frame>
        {wizardEl}
        {detailEl}
      </>
    );
  }
  if (!model) return null;
  return (
    <>
      <ModelPage
        key={model.name}
        app={app}
        model={model}
        route={{ ...route, model: model.name }}
        go={go}
        extra={
          model.name === "connection" ? (
            <Button variant="primary" icon="plus" onClick={() => openWizard()}>
              {t("Connect a service")}
            </Button>
          ) : undefined
        }
        onNew={model.name === "connection" ? () => openWizard() : undefined}
      />
      {wizardEl}
      {detailEl}
    </>
  );
}
