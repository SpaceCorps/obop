import { useEffect, useMemo, useState } from "react";
import { Badge, Table, fmt, t } from "@obop/ui";
import { call } from "../api.ts";
import { useData, useRows } from "../data.tsx";
import { modelTitle } from "../labels.ts";
import { serverMessage } from "../serverMessage.ts";
import { Button, EmptyState, Notice, Select, Skeleton, Stat } from "../ui.tsx";
import { FieldPicker } from "./FieldPicker.tsx";
import { PrivacyPreview } from "./PrivacyPreview.tsx";
import {
  dayStart,
  directionLabel,
  basisLabel,
  downloadText,
  parseJson,
  stateLabel,
  stateTone,
  type Connection,
  type Flow,
  type Provider,
  type Run,
} from "./model.ts";

const when = (iso: string) => {
  const ms = Date.parse(iso);
  return Number.isNaN(ms) ? iso : fmt.dateTime(new Date(ms));
};

type CheckItem = {
  id: string;
  status: "ok" | "warn" | "todo" | "info";
  title: string;
  detail: string;
  subjects: string[];
};
type Checklist = { disclaimer: string; items: CheckItem[]; generated_at: string };

const statusTone = (s: string) =>
  s === "ok" ? "success" : s === "warn" ? "warning" : s === "todo" ? "danger" : ("info" as const);
const statusLabel = (s: string) =>
  s === "ok" ? t("In place") : s === "warn" ? t("Check") : s === "todo" ? t("To do") : t("Note");

/** The checklist an administrator reads: configuration the instance can see, never a verdict. */
export function useChecklist(enabled: boolean) {
  const d = useData();
  const [state, setState] = useState<{ list?: Checklist; error?: string }>({});
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    void call(d.token, "egress_policy.checklist", {}).then((r) => {
      if (!live) return;
      if (r.kind === "ok")
        setState({
          list: parseJson<Checklist>((r.value as { result: string }).result, undefined as never),
        });
      else setState({ error: r.kind === "error" ? serverMessage(r.message) : t("Needs approval") });
    });
    return () => {
      live = false;
    };
  }, [d.token, enabled, d.mutations]);
  return state;
}

export function Compliance() {
  const { list, error } = useChecklist(true);
  return (
    <div className="cx-page">
      <Notice tone="info" title={t("What this checklist is")}>
        {list?.disclaimer ??
          t(
            "It lists technical controls and settings that support GDPR compliance. It is not legal advice, not a certification and not a statement that your processing is lawful.",
          )}
      </Notice>
      {error ? (
        <EmptyState title={t("Could not load the checklist")} hint={error} />
      ) : !list ? (
        <Skeleton rows={6} cols={2} />
      ) : (
        <ul className="cx-checks" aria-label={t("Checklist")}>
          {list.items.map((i) => (
            <li key={i.id} data-status={i.status}>
              <Badge tone={statusTone(i.status)}>{statusLabel(i.status)}</Badge>
              <div>
                <strong>{i.title}</strong>
                <p>{i.detail}</p>
                {i.subjects.length > 0 && <p className="hint">{i.subjects.join(", ")}</p>}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function Register() {
  const d = useData();
  const rows = useRows("processing_register");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function exportCsv() {
    setBusy(true);
    setError(null);
    const r = await call(d.token, "processing_register.export_csv", {});
    setBusy(false);
    if (r.kind === "ok") downloadText("processing-register.csv", (r.value as { csv: string }).csv);
    else setError(r.kind === "error" ? serverMessage(r.message) : t("Needs approval"));
  }
  return (
    <div className="cx-page">
      <Notice tone="info" title={t("A starting point for your record of processing activities")}>
        {t(
          "The register is generated from the connections, so it cannot drift from what is configured. It supports the record required by Article 30 GDPR; it does not replace your own record, and it does not cover processing outside connectors.",
        )}
      </Notice>
      <div className="actionrow">
        <Button variant="primary" icon="download" loading={busy} onClick={() => void exportCsv()}>
          {t("Export as CSV")}
        </Button>
      </div>
      {error && (
        <Notice tone="danger" title={t("Could not export")}>
          {error}
        </Notice>
      )}
      {!rows.loaded ? (
        <Skeleton rows={4} cols={5} />
      ) : rows.rows.length === 0 ? (
        <EmptyState
          title={t("Nothing to record yet")}
          hint={t("Connections appear here as soon as they exist.")}
        />
      ) : (
        <Table caption={t("Processing register")}>
          <thead>
            <tr>
              <th scope="col">{t("Connection")}</th>
              <th scope="col">{t("Recipient")}</th>
              <th scope="col">{t("Purpose")}</th>
              <th scope="col">{t("Legal basis")}</th>
              <th scope="col">{t("Personal data")}</th>
              <th scope="col">{t("Transfers")}</th>
              <th scope="col">{t("Agreement")}</th>
              <th scope="col">{t("Retention")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.rows.map((r) => (
              <tr key={r.id}>
                <th scope="row">
                  {String(r.name)}
                  <span className="hint block">{stateLabel(String(r.state))}</span>
                </th>
                <td>{String(r.recipient)}</td>
                <td>{String(r.purpose)}</td>
                <td>
                  {String(r.legal_basis) === "not recorded"
                    ? t("Not recorded")
                    : basisLabel(String(r.legal_basis))}
                </td>
                <td>{String(r.personal_data_categories) || "—"}</td>
                <td>{String(r.transfer_basis)}</td>
                <td>
                  {String(r.agreement_recorded) === "yes" ? t("Recorded") : t("Not recorded")}
                </td>
                <td>
                  {t("{count, plural, one {# day} other {# days}}", {
                    count: Number(r.retention_days),
                  })}
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}
    </div>
  );
}

export function PreviewPage() {
  const d = useData();
  const providers = useRows("connector_provider");
  const [provider, setProvider] = useState("");
  const [allow, setAllow] = useState<Record<string, string[]>>({});
  const [model, setModel] = useState("");
  const list = (providers.rows as unknown as Provider[]).filter(
    (p) => p.maturity !== "definition_only" && p.direction !== "inbound",
  );
  const chosen = list.find((p) => p.id === provider) ?? list[0];
  const models = useMemo(() => [...d.models.values()], [d.models]);
  const names = Object.keys(allow).filter((m) => allow[m]!.length);
  const active = names.includes(model) ? model : (names[0] ?? "");
  if (!providers.loaded) return <Skeleton rows={4} cols={2} />;
  if (!chosen)
    return (
      <EmptyState
        title={t("No service to preview")}
        hint={t("An administrator enables a service in the catalogue first.")}
      />
    );
  return (
    <div className="cx-page cx-split">
      <div>
        <Select
          label={t("Service")}
          value={chosen.id}
          onChange={(e) => setProvider(e.target.value)}
        >
          {list.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </Select>
        <FieldPicker
          models={models}
          providerMax={chosen.max_classes.split(",").filter(Boolean)}
          inbound={false}
          value={allow}
          onChange={setAllow}
          raised={[]}
        />
        {names.length > 1 && (
          <Select
            label={t("Preview this model")}
            value={active}
            onChange={(e) => setModel(e.target.value)}
          >
            {names.map((m) => {
              const info = d.models.get(m);
              return (
                <option key={m} value={m}>
                  {info ? modelTitle(info.model, info.module.id) : m}
                </option>
              );
            })}
          </Select>
        )}
      </div>
      <div>
        {active ? (
          <PrivacyPreview provider={chosen.id} model={active} allow={allow} policy="" />
        ) : (
          <EmptyState
            title={t("Choose some fields")}
            hint={t("The preview shows one real record as it would leave.")}
          />
        )}
      </div>
    </div>
  );
}

export function Overview(props: {
  isAdmin: boolean;
  onConnect: () => void;
  onOpen: (id: string) => void;
  goCompliance: () => void;
}) {
  const conns = useRows("connection");
  const runs = useRows("sync_run");
  const flows = useRows("data_flow_log");
  const providers = useRows("connector_provider");
  const { list } = useChecklist(props.isAdmin);
  const mine = conns.rows as unknown as Connection[];
  const today = dayStart();
  const flowsToday = (flows.rows as unknown as Flow[]).filter(
    (f) => f.at.startsWith(today) && (f.outcome === "released" || f.outcome === "received"),
  );
  const recordsToday = flowsToday.reduce((n, f) => n + Number(f.record_count || 0), 0);
  const attention = mine.filter((c) => ["draft", "paused", "error"].includes(c.state));
  const lastRuns = (runs.rows as unknown as Run[])
    .slice()
    .sort((a, b) => b.id.localeCompare(a.id))
    .slice(0, 8);
  const pname = (id: string) =>
    (providers.rows as unknown as Provider[]).find((p) => p.id === id)?.name ?? id;
  const cname = (id: string) => mine.find((c) => c.id === id)?.name ?? id.slice(0, 8);
  const warnings = (list?.items ?? [])
    .filter((i) => i.status === "warn" || i.status === "todo")
    .slice(0, 4);
  const loaded = conns.loaded && runs.loaded && flows.loaded;
  if (!loaded) return <Skeleton rows={6} cols={4} />;
  return (
    <div className="cx-page">
      <div className="cx-stats">
        <Stat
          label={t("Connected")}
          value={fmt.integer(mine.filter((c) => c.state === "connected").length)}
          tone="success"
        />
        <Stat
          label={t("Needs attention")}
          value={fmt.integer(attention.length)}
          tone={attention.some((c) => c.state === "error") ? "danger" : "neutral"}
          hint={t("Drafts, paused and failing connections")}
        />
        <Stat label={t("Flows today")} value={fmt.integer(flowsToday.length)} />
        <Stat label={t("Records today")} value={fmt.integer(recordsToday)} />
      </div>
      {warnings.length > 0 && (
        <Notice
          tone="warning"
          title={t("{count, plural, one {# thing to look at} other {# things to look at}}", {
            count: warnings.length,
          })}
          action={
            <Button size="sm" onClick={props.goCompliance}>
              {t("Open the checklist")}
            </Button>
          }
        >
          {warnings.map((w) => w.title).join(" · ")}
        </Notice>
      )}
      <section aria-label={t("Your connections")}>
        <div className="cx-head">
          <h2>{props.isAdmin ? t("Connections") : t("Your connections")}</h2>
          <Button variant="primary" icon="plus" onClick={props.onConnect}>
            {t("Connect a service")}
          </Button>
        </div>
        {mine.length === 0 ? (
          <EmptyState
            icon="plug"
            title={t("Nothing is connected yet")}
            hint={t(
              "Connect a service from the catalogue. Nothing leaves until you record why and for what.",
            )}
            action={{ label: t("Connect a service"), onClick: props.onConnect }}
          />
        ) : (
          <ul className="cx-cards compact" aria-label={t("Connections")}>
            {mine.map((c) => (
              <li key={c.id} className="cx-card">
                <header>
                  <h3>{c.name}</h3>
                  <Badge tone={stateTone(c.state)}>{stateLabel(c.state)}</Badge>
                </header>
                <p className="hint">
                  {pname(c.provider)} ·{" "}
                  {c.last_sync_at
                    ? t("last ran {when}", { when: when(c.last_sync_at) })
                    : t("has not run yet")}
                </p>
                {c.last_error && <p className="cx-err">{c.last_error}</p>}
                <footer className="actionrow">
                  <Button size="sm" onClick={() => props.onOpen(c.id)}>
                    {t("Open")}
                  </Button>
                </footer>
              </li>
            ))}
          </ul>
        )}
      </section>
      <section aria-label={t("Latest runs")}>
        <h2>{t("Latest runs")}</h2>
        {lastRuns.length === 0 ? (
          <p className="hint">{t("Nothing has run yet.")}</p>
        ) : (
          <Table dense caption={t("Latest runs")}>
            <thead>
              <tr>
                <th scope="col">{t("Connection")}</th>
                <th scope="col">{t("Direction")}</th>
                <th scope="col">{t("Started")}</th>
                <th scope="col">{t("Status")}</th>
                <th scope="col">{t("Records")}</th>
              </tr>
            </thead>
            <tbody>
              {lastRuns.map((r) => (
                <tr key={r.id}>
                  <th scope="row">{cname(r.connection)}</th>
                  <td>{directionLabel(r.direction)}</td>
                  <td>{when(r.started_at)}</td>
                  <td>
                    {r.status}
                    {r.error_summary && <span className="hint block">{r.error_summary}</span>}
                  </td>
                  <td>{fmt.integer(Number(r.records))}</td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </section>
    </div>
  );
}
