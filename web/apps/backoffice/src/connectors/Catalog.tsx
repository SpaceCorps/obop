import { useState } from "react";
import { Badge, t } from "@obop/ui";
import type { App } from "../Chrome.tsx";
import { useData, useRows } from "../data.tsx";
import { serverMessage } from "../serverMessage.ts";
import {
  Button,
  Dialog,
  EmptyState,
  Input,
  Notice,
  Select,
  Skeleton,
  Switch,
  Textarea,
} from "../ui.tsx";
import {
  adequacyLabel,
  basisLabel,
  className,
  directionLabel,
  maturityLabel,
  maturityTone,
  type Provider,
} from "./model.ts";

const BASES = ["consent", "contract", "legitimate_interest", "legal_obligation"];

type PolicyRow = {
  id: string;
  provider: string;
  enabled?: boolean;
  users_may_use?: boolean;
  dpa_recorded?: boolean;
  dpa_reference?: string;
  legal_basis?: string;
  legal_basis_text?: string;
  allowed_group?: string;
  oauth_client_id?: string;
};

function PolicyDialog({
  provider,
  existing,
  onClose,
}: {
  provider: Provider;
  existing?: PolicyRow;
  onClose: () => void;
}) {
  const d = useData();
  const [v, setV] = useState({
    enabled: existing?.enabled ?? true,
    users_may_use: existing?.users_may_use ?? false,
    dpa_recorded: existing?.dpa_recorded ?? false,
    dpa_reference: existing?.dpa_reference ?? "",
    legal_basis: existing?.legal_basis ?? "",
    legal_basis_text: existing?.legal_basis_text ?? "",
    oauth_client_id: existing?.oauth_client_id ?? "",
    oauth_client_secret: "",
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function save() {
    setBusy(true);
    setError(null);
    const body: Record<string, unknown> = { ...v };
    if (!v.legal_basis) delete body.legal_basis;
    if (!v.oauth_client_secret) delete body.oauth_client_secret;
    if (provider.auth_kind !== "oauth2") {
      delete body.oauth_client_id;
      delete body.oauth_client_secret;
    }
    const r = existing
      ? await d.run("provider_policy.update", { id: existing.id, ...body })
      : await d.run("provider_policy.create", { provider: provider.id, ...body });
    setBusy(false);
    if (r.kind === "ok") {
      d.notify(t("Policy saved"));
      await d.store.load("connector_provider");
      return onClose();
    }
    setError(r.kind === "approval" ? t("Approval needed.") : serverMessage(r.message));
  }
  return (
    <Dialog
      title={t("Policy for {provider}", { provider: provider.name })}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" loading={busy} onClick={() => void save()}>
            {t("Save")}
          </Button>
        </>
      }
    >
      <div className="wform">
        {error && (
          <Notice tone="danger" title={t("Could not save")}>
            {error}
          </Notice>
        )}
        <Switch
          label={t("This service may be connected")}
          checked={v.enabled}
          onChange={(e) => setV({ ...v, enabled: e.target.checked })}
        />
        <Switch
          label={t("People may connect their own accounts")}
          checked={v.users_may_use}
          onChange={(e) => setV({ ...v, users_may_use: e.target.checked })}
        />
        <Switch
          label={t("The agreement with the provider is in place")}
          checked={v.dpa_recorded}
          onChange={(e) => setV({ ...v, dpa_recorded: e.target.checked })}
        />
        <Input
          label={t("Where the agreement is kept")}
          value={v.dpa_reference}
          hint={t(
            "A reference or a place, for example “DPA signed 2026-10-01, folder Legal”. obop does not check the agreement.",
          )}
          onChange={(e) => setV({ ...v, dpa_reference: e.target.value })}
        />
        <Select
          label={t("Legal basis for people’s own connections")}
          value={v.legal_basis}
          onChange={(e) => setV({ ...v, legal_basis: e.target.value })}
        >
          <option value="">{t("Let people choose")}</option>
          {BASES.map((b) => (
            <option key={b} value={b}>
              {basisLabel(b)}
            </option>
          ))}
        </Select>
        <Textarea
          label={t("Text shown when people are asked")}
          rows={3}
          value={v.legal_basis_text}
          onChange={(e) => setV({ ...v, legal_basis_text: e.target.value })}
        />
        {provider.auth_kind === "oauth2" && (
          <>
            <Notice tone="info" title={t("Needs your own OAuth client")}>
              {t(
                "obop ships no client id or secret. Register an application with the provider and enter its credentials here.",
              )}
            </Notice>
            <Input
              label={t("OAuth client id")}
              value={v.oauth_client_id}
              onChange={(e) => setV({ ...v, oauth_client_id: e.target.value })}
            />
            <Input
              label={t("OAuth client secret")}
              type="password"
              autoComplete="off"
              value={v.oauth_client_secret}
              hint={t(
                "Write-only: it is stored encrypted and never shown again. Leave empty to keep the stored one.",
              )}
              onChange={(e) => setV({ ...v, oauth_client_secret: e.target.value })}
            />
          </>
        )}
      </div>
    </Dialog>
  );
}

/** The provider catalogue: what exists, how much of it works, and who the recipient is. */
export function Catalog({
  app,
  isAdmin,
  onConnect,
}: {
  app: App;
  isAdmin: boolean;
  onConnect: (id: string) => void;
}) {
  const d = useData();
  const providers = useRows("connector_provider");
  const policies = useRows("provider_policy", isAdmin);
  const [maturity, setMaturity] = useState("");
  const [direction, setDirection] = useState("");
  const [editing, setEditing] = useState<Provider | null>(null);
  const rows = (providers.rows as unknown as Provider[]).filter(
    (p) =>
      (!maturity || p.maturity === maturity) &&
      (!direction || p.direction === direction || p.direction === "both"),
  );
  if (!providers.loaded) return <Skeleton rows={6} cols={3} />;
  return (
    <div className="cx-page">
      <p className="hint">
        {t(
          "Every entry says how much of it exists. Working means implemented and tested end to end against a local counterpart. A generic adapter configures a generic engine for the service and is not tested against the service itself. A definition only describes the service: nothing runs.",
        )}
      </p>
      <div className="actionrow">
        <Select
          label={t("How much exists")}
          value={maturity}
          onChange={(e) => setMaturity(e.target.value)}
        >
          <option value="">{t("All")}</option>
          <option value="working">{t("Working")}</option>
          <option value="generic_adapter">{t("Generic adapter")}</option>
          <option value="definition_only">{t("Definition only")}</option>
        </Select>
        <Select
          label={t("Direction")}
          value={direction}
          onChange={(e) => setDirection(e.target.value)}
        >
          <option value="">{t("All")}</option>
          <option value="outbound">{t("Outbound")}</option>
          <option value="inbound">{t("Inbound")}</option>
        </Select>
      </div>
      {rows.length === 0 ? (
        <EmptyState title={t("No service matches")} hint={t("Clear a filter to see more.")} />
      ) : (
        <ul className="cx-cards" aria-label={t("Services")}>
          {rows.map((p) => {
            const usable = p.maturity !== "definition_only";
            return (
              <li key={p.id} className="cx-card" data-provider={p.id}>
                <header>
                  <h3>{p.name}</h3>
                  <span className="cx-badges">
                    <Badge tone={maturityTone(p.maturity)}>{maturityLabel(p.maturity)}</Badge>
                    <Badge tone="neutral" dot={false}>
                      {directionLabel(p.direction)}
                    </Badge>
                  </span>
                </header>
                <p>{p.purpose}</p>
                <p className="hint">{p.description}</p>
                <dl className="cx-facts">
                  <dt>{t("Receives at most")}</dt>
                  <dd>
                    {p.max_classes
                      .split(",")
                      .filter(Boolean)
                      .map((c) => className(c))
                      .join(", ")}
                  </dd>
                  <dt>{t("Recipient")}</dt>
                  <dd>{p.legal_name}</dd>
                  <dt>{t("Country")}</dt>
                  <dd>{p.country}</dd>
                  <dt>{t("Transfers")}</dt>
                  <dd>{adequacyLabel(p.eu_adequate)}</dd>
                  <dt>{t("Agreement")}</dt>
                  <dd>
                    {p.dpa_recorded ? t("Recorded") : t("Not recorded")}
                    {p.dpa_url && (
                      <>
                        {" · "}
                        <a href={p.dpa_url} target="_blank" rel="noopener noreferrer">
                          {t("Provider’s terms")}
                        </a>
                      </>
                    )}
                  </dd>
                </dl>
                <p className="hint">{p.verify_note}</p>
                <footer className="actionrow">
                  {usable && p.enabled && (
                    <Button variant="primary" onClick={() => onConnect(p.id)}>
                      {t("Connect")}
                    </Button>
                  )}
                  {usable && !p.enabled && !isAdmin && (
                    <span className="hint">{t("Not enabled: ask an administrator.")}</span>
                  )}
                  {!usable && (
                    <span className="hint">
                      {t("Nothing runs yet: this entry is a definition.")}
                    </span>
                  )}
                  {isAdmin && usable && (
                    <Button onClick={() => setEditing(p)}>
                      {p.enabled ? t("Policy") : t("Enable")}
                    </Button>
                  )}
                  {isAdmin && !usable && p.auth_kind === "oauth2" && (
                    <Button onClick={() => setEditing(p)}>{t("Policy")}</Button>
                  )}
                </footer>
              </li>
            );
          })}
        </ul>
      )}
      <p className="hint">
        {t("{count, plural, one {# service} other {# services}} listed in {app}.", {
          count: rows.length,
          app: app.name,
        })}
      </p>
      {editing && (
        <PolicyDialog
          provider={editing}
          existing={(policies.rows as unknown as PolicyRow[]).find(
            (x) => x.provider === editing.id,
          )}
          onClose={() => {
            setEditing(null);
            void d.store.load("provider_policy");
          }}
        />
      )}
    </div>
  );
}
