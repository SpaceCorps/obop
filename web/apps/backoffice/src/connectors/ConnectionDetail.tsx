import { useState } from "react";
import { Badge, Table, fmt, t } from "@obop/ui";
import { useData, useRows } from "../data.tsx";
import { serverMessage } from "../serverMessage.ts";
import { Button, Dialog, Notice } from "../ui.tsx";
import {
  basisLabel,
  className,
  directionLabel,
  downloadText,
  parseJson,
  stateLabel,
  stateTone,
  type Connection,
  type Consent,
  type Flow,
  type Provider,
  type Run,
} from "./model.ts";

const when = (iso: string) => {
  const ms = Date.parse(iso);
  return Number.isNaN(ms) ? iso : fmt.dateTime(new Date(ms));
};

/** One connection: its state, the legal basis, what it did lately, and the actions on it. */
export function ConnectionDetail({ id, onClose }: { id: string; onClose: () => void }) {
  const d = useData();
  const conns = useRows("connection");
  const providers = useRows("connector_provider");
  const consents = useRows("consent");
  const runs = useRows("sync_run");
  const flows = useRows("data_flow_log");
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState<{
    tone: "success" | "danger" | "info";
    text: string;
  } | null>(null);

  const c = (conns.rows as unknown as Connection[]).find((x) => x.id === id);
  const provider = (providers.rows as unknown as Provider[]).find((p) => p.id === c?.provider);
  const basis = (consents.rows as unknown as Consent[])
    .filter((k) => k.connection === id)
    .sort((a, b) => b.id.localeCompare(a.id));
  const active = basis.find((k) => k.state === "active");
  const myRuns = (runs.rows as unknown as Run[])
    .filter((r) => r.connection === id)
    .sort((a, b) => b.id.localeCompare(a.id))
    .slice(0, 5);
  const myFlows = (flows.rows as unknown as Flow[])
    .filter((f) => f.connection === id)
    .sort((a, b) => b.id.localeCompare(a.id))
    .slice(0, 5);

  const refresh = async () => {
    await Promise.all(
      ["connection", "consent", "sync_run", "data_flow_log"].map((m) => d.store.load(m)),
    );
  };
  async function act(command: string, label: string, body: Record<string, unknown> = { id }) {
    setBusy(command);
    setMessage(null);
    const r = await d.run(command, body);
    setBusy("");
    await refresh();
    if (r.kind === "approval")
      return setMessage({
        tone: "info",
        text: t(
          "Approval needed: {action} was proposed as plan {plan}. Nothing changes until a human approves it.",
          {
            action: label,
            plan: r.planId.slice(0, 8),
          },
        ),
      });
    if (r.kind === "error") return setMessage({ tone: "danger", text: serverMessage(r.message) });
    const result = parseJson<Record<string, unknown>>((r.value as { result?: string }).result, {});
    if (typeof result.csv === "string") {
      downloadText(String(result.filename ?? "export.csv"), result.csv);
      return setMessage({
        tone: "success",
        text: t("The file was downloaded: {count, plural, one {# record} other {# records}}.", {
          count: Number(result.records ?? 0),
        }),
      });
    }
    if (Array.isArray(result.checks))
      return setMessage({ tone: "success", text: (result.checks as string[]).join(". ") });
    if (typeof result.url === "string") {
      window.open(result.url, "_blank", "noopener,noreferrer");
      return setMessage({
        tone: "info",
        text: t("The authorisation page opened in a new window."),
      });
    }
    setMessage({ tone: "success", text: t("Done.") });
  }

  async function withdraw() {
    if (!active) return;
    const ok = await d.confirm(
      t(
        "Withdraw the legal basis for “{name}”? The connection is revoked and its stored credentials and tokens are removed. This cannot be undone.",
        { name: c?.name ?? "" },
      ),
      t("Withdraw and disconnect"),
      t("Keep it"),
    );
    if (ok) await act("consent.withdraw", t("withdrawing the legal basis"), { id: active.id });
  }

  if (!c) return null;
  const canSync = provider && ["webhook_out", "rest_pull", "csv_export"].includes(provider.engine);
  const hook = provider?.engine === "webhook_in" ? `${location.origin}/hooks/in/${c.id}` : "";
  const classes = (active?.classes ?? "").split(",").filter(Boolean);
  return (
    <Dialog title={c.name} size="lg" onClose={onClose}>
      <div className="cx-detail">
        <p>
          <Badge tone={stateTone(c.state)}>{stateLabel(c.state)}</Badge>{" "}
          {provider && (
            <>
              {provider.name} · {directionLabel(provider.direction)}
            </>
          )}
        </p>
        {message && (
          <Notice
            tone={message.tone}
            title={message.tone === "danger" ? t("This did not work") : undefined}
          >
            {message.text}
          </Notice>
        )}
        {c.last_error && (
          <Notice tone="warning" title={t("Last problem")}>
            {c.last_error}
          </Notice>
        )}
        <div className="actionrow">
          {c.state === "draft" && (
            <Button
              variant="primary"
              loading={busy === "connection.connect"}
              onClick={() => void act("connection.connect", t("connecting"))}
            >
              {t("Connect")}
            </Button>
          )}
          {c.state === "connected" && (
            <Button
              loading={busy === "connection.pause"}
              onClick={() => void act("connection.pause", t("pausing"))}
            >
              {t("Pause")}
            </Button>
          )}
          {c.state === "paused" && (
            <Button
              variant="primary"
              loading={busy === "connection.resume"}
              onClick={() => void act("connection.resume", t("resuming"))}
            >
              {t("Resume")}
            </Button>
          )}
          {c.state === "error" && (
            <Button
              variant="primary"
              loading={busy === "connection.retry"}
              onClick={() => void act("connection.retry", t("retrying"))}
            >
              {t("Retry")}
            </Button>
          )}
          {c.state !== "revoked" && (
            <Button
              loading={busy === "connection.test"}
              onClick={() => void act("connection.test", t("testing"))}
            >
              {t("Test connection")}
            </Button>
          )}
          {c.state === "connected" && canSync && (
            <Button
              loading={busy === "connection.sync"}
              onClick={() => void act("connection.sync", t("running now"))}
            >
              {provider?.engine === "csv_export"
                ? t("Download the file")
                : provider?.engine === "rest_pull"
                  ? t("Pull now")
                  : t("Send now")}
            </Button>
          )}
          {provider?.auth_kind === "oauth2" && c.state !== "revoked" && (
            <Button onClick={() => void act("connection.oauth_start", t("authorising"))}>
              {t("Authorise with the provider")}
            </Button>
          )}
          {c.state !== "revoked" && (
            <Button
              variant="danger"
              loading={busy === "connection.revoke"}
              onClick={async () => {
                if (
                  await d.confirm(
                    t("Revoke “{name}”? Access stops and the stored credentials are removed.", {
                      name: c.name,
                    }),
                    t("Revoke"),
                    t("Keep it"),
                  )
                )
                  await act("connection.revoke", t("revoking"));
              }}
            >
              {t("Revoke")}
            </Button>
          )}
        </div>

        <section aria-label={t("Legal basis")}>
          <h3>{t("Legal basis")}</h3>
          {active ? (
            <>
              <dl className="cx-facts">
                <dt>{t("Basis")}</dt>
                <dd>{basisLabel(active.legal_basis)}</dd>
                <dt>{t("Purpose")}</dt>
                <dd>{active.purpose}</dd>
                <dt>{t("Data classes")}</dt>
                <dd>
                  {classes.length
                    ? classes.map((x) => className(x)).join(", ")
                    : t("None beyond non-personal data")}
                </dd>
                <dt>{t("Recorded")}</dt>
                <dd>{when(active.recorded_at)}</dd>
              </dl>
              {c.state !== "revoked" && (
                <Button
                  variant="danger"
                  loading={busy === "consent.withdraw"}
                  onClick={() => void withdraw()}
                >
                  {t("Withdraw and disconnect")}
                </Button>
              )}
            </>
          ) : basis.length ? (
            <p>
              {t("The legal basis was withdrawn on {date}.", {
                date: when(basis[0]!.withdrawn_at || basis[0]!.recorded_at),
              })}
            </p>
          ) : (
            <p>{t("No legal basis is recorded, so nothing flows.")}</p>
          )}
        </section>

        <section aria-label={t("Settings")}>
          <h3>{t("Settings")}</h3>
          <dl className="cx-facts">
            <dt>{t("Credentials")}</dt>
            <dd>{c.credentials_hint || t("None")}</dd>
            <dt>{t("Retention")}</dt>
            <dd>
              {t("{count, plural, one {# day} other {# days}}", {
                count: Number(c.retention_days),
              })}
            </dd>
            {hook && (
              <>
                <dt>{t("Webhook address")}</dt>
                <dd>
                  <code>{hook}</code>
                </dd>
              </>
            )}
          </dl>
        </section>

        <section aria-label={t("Recent runs")}>
          <h3>{t("Recent runs")}</h3>
          {myRuns.length === 0 ? (
            <p className="hint">{t("Nothing has run yet.")}</p>
          ) : (
            <Table dense caption={t("Recent runs")}>
              <thead>
                <tr>
                  <th scope="col">{t("Started")}</th>
                  <th scope="col">{t("Status")}</th>
                  <th scope="col">{t("Records")}</th>
                </tr>
              </thead>
              <tbody>
                {myRuns.map((r) => (
                  <tr key={r.id}>
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

        <section aria-label={t("Recent data flows")}>
          <h3>{t("Recent data flows")}</h3>
          {myFlows.length === 0 ? (
            <p className="hint">{t("No data has flowed yet.")}</p>
          ) : (
            <Table dense caption={t("Recent data flows")}>
              <thead>
                <tr>
                  <th scope="col">{t("When")}</th>
                  <th scope="col">{t("Outcome")}</th>
                  <th scope="col">{t("Fields")}</th>
                </tr>
              </thead>
              <tbody>
                {myFlows.map((f) => (
                  <tr key={f.id}>
                    <td>{when(f.at)}</td>
                    <td>
                      {f.outcome}
                      {f.reason && <span className="hint block">{f.reason}</span>}
                    </td>
                    <td>{f.fields_sent || "—"}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}
        </section>
      </div>
    </Dialog>
  );
}
