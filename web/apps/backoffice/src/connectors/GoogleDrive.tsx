import { useEffect, useState } from "react";
import { Badge, t } from "@obop/ui";
import type { Reply } from "../api.ts";
import { useData, useRows } from "../data.tsx";
import { request } from "../net.ts";
import { serverMessage } from "../serverMessage.ts";
import { Button, Input, Notice, Select, Skeleton } from "../ui.tsx";
import { parseJson } from "./model.ts";

/* The Google Drive page: an administrator enters the OAuth client of their own Google Cloud
   project, signs in with Google, and the Drive browser of the Documents app works. Everything goes
   through the commands of the Connectors app (provider_policy, connection.oauth_start,
   connection.revoke); the client secret is sealed on the server and never shown again. */

const PROVIDER = "google_drive";

type DriveSettings = {
  state: "not_configured" | "needs_consent" | "connected" | "error";
  client_id: string;
  secret_set: boolean;
  account: string;
  redirect_uri: string | null;
  connection: string | null;
  last_error: string;
  browse_group: string;
  workspace_id: string;
  workspace_name: string;
  write_group: string;
  write_granted: boolean;
  source: "admin" | "env";
};

type SharedDrive = { id: string; name: string };

type PolicyRow = { id: string; provider: string };

function useDriveSettings(token: string, version: number) {
  const [state, setState] = useState<{ settings?: DriveSettings; error?: string }>({});
  const [again, setAgain] = useState(0);
  useEffect(() => {
    let live = true;
    request("/api/drive/status", { headers: { authorization: `Bearer ${token}` } }, { retry: true })
      .then(async (r) => {
        if (!r.ok) throw new Error(String(r.status));
        return (await r.json()) as { settings?: DriveSettings };
      })
      .then((v) => live && setState({ settings: v.settings }))
      .catch(() => live && setState({ error: t("Could not read the state of the connection.") }));
    return () => {
      live = false;
    };
  }, [token, version, again]);
  return { ...state, reload: () => setAgain((n) => n + 1) };
}

const tone = (s: DriveSettings["state"]) =>
  s === "connected"
    ? "success"
    : s === "error"
      ? "danger"
      : s === "needs_consent"
        ? "warning"
        : ("neutral" as const);

const label = (s: DriveSettings["state"]) =>
  s === "connected"
    ? t("Connected")
    : s === "error"
      ? t("Problem")
      : s === "needs_consent"
        ? t("Needs sign-in with Google")
        : t("Not set up");

/** The page: status, the steps to register the client with Google, the form and the buttons. */
export function GoogleDrive() {
  const d = useData();
  const policies = useRows("provider_policy");
  const { settings, error, reload } = useDriveSettings(d.token, d.mutations);
  const [clientId, setClientId] = useState<string | null>(null);
  const [secret, setSecret] = useState("");
  const [group, setGroup] = useState<string | null>(null);
  const [writeGroup, setWriteGroup] = useState<string | null>(null);
  const [workspace, setWorkspace] = useState<string | null>(null);
  const [drives, setDrives] = useState<SharedDrive[] | null>(null);
  const [drivesError, setDrivesError] = useState("");
  const [drivesAgain, setDrivesAgain] = useState(0);
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState<{
    tone: "success" | "danger" | "info";
    text: string;
  } | null>(null);

  const connected = settings?.state === "connected";
  useEffect(() => {
    if (!connected) return;
    let live = true;
    setDrivesError("");
    request(
      "/api/drive/drives",
      { headers: { authorization: `Bearer ${d.token}` } },
      { retry: true },
    )
      .then(async (r) => {
        if (!r.ok) throw new Error(String(r.status));
        return (await r.json()) as { drives?: SharedDrive[] };
      })
      .then((v) => live && setDrives(v.drives ?? []))
      .catch(() => live && setDrivesError(t("Could not read your shared drives.")));
    return () => {
      live = false;
    };
  }, [connected, d.token, drivesAgain]);

  if (error) return <Notice tone="danger">{error}</Notice>;
  if (!settings) return <Skeleton rows={6} cols={2} />;

  const id = clientId ?? settings.client_id;
  const browseGroup = group ?? settings.browse_group;
  const writeGroupValue = writeGroup ?? settings.write_group;
  const workspaceId = workspace ?? settings.workspace_id;
  const policy = (policies.rows as unknown as PolicyRow[]).find((p) => p.provider === PROVIDER);
  const configured = settings.state !== "not_configured";
  const redirect = settings.redirect_uri;

  const fail = (r: Reply): boolean => {
    if (r.kind === "approval") {
      setMessage({
        tone: "info",
        text: t(
          "Approval needed: this was proposed as plan {plan}. Nothing changes until a human approves it.",
          {
            plan: r.planId.slice(0, 8),
          },
        ),
      });
      return true;
    }
    if (r.kind === "error") {
      setMessage({ tone: "danger", text: serverMessage(r.message) });
      return true;
    }
    return false;
  };

  async function save() {
    setBusy("save");
    setMessage(null);
    const body: Record<string, unknown> = {
      enabled: true,
      oauth_client_id: id.trim(),
      allowed_group: browseGroup.trim() === "platform.admin" ? "" : browseGroup.trim(),
      write_group: writeGroupValue.trim() === "platform.admin" ? "" : writeGroupValue.trim(),
    };
    if (secret) body.oauth_client_secret = secret;
    const r = policy
      ? await d.run("provider_policy.update", { id: policy.id, ...body })
      : await d.run("provider_policy.create", { provider: PROVIDER, ...body });
    setBusy("");
    if (fail(r)) return;
    setSecret("");
    setClientId(null);
    setGroup(null);
    setWriteGroup(null);
    await d.store.load("provider_policy");
    reload();
    setMessage({ tone: "success", text: t("Saved.") });
  }

  async function saveWorkspace() {
    if (!policy) return;
    setBusy("workspace");
    setMessage(null);
    const name =
      workspaceId === ""
        ? ""
        : workspaceId === "root"
          ? t("My Drive")
          : ((drives ?? []).find((x) => x.id === workspaceId)?.name ?? "");
    const r = await d.run("provider_policy.update", {
      id: policy.id,
      workspace_id: workspaceId,
      workspace_name: name,
    });
    setBusy("");
    if (fail(r)) return;
    setWorkspace(null);
    await d.store.load("provider_policy");
    reload();
    setMessage({ tone: "success", text: t("Workspace saved.") });
  }

  async function connect() {
    setBusy("connect");
    setMessage(null);
    let conn = settings?.connection ?? null;
    if (!conn) {
      const made = await d.run("connection.create", {
        name: "Google Drive",
        provider: PROVIDER,
        owner_scope: "instance",
      });
      if (fail(made)) return setBusy("");
      conn = (made.kind === "ok" ? (made.value as { id?: string }).id : undefined) ?? null;
    }
    const r = await d.run("connection.oauth_start", { id: conn });
    setBusy("");
    if (fail(r)) return;
    const link = parseJson<{ url?: string }>(
      (r.kind === "ok" ? (r.value as { result?: string }).result : "") ?? "",
      {},
    ).url;
    if (!link)
      return setMessage({ tone: "danger", text: t("Google did not give a sign-in address.") });
    // Google asks the person to agree, then sends the browser back to obop.
    window.location.assign(link);
  }

  async function disconnect() {
    if (!settings?.connection) return;
    const ok = await d.confirm(
      t(
        "Disconnect Google Drive? obop forgets the sign-in and nobody can browse the Drive until you connect again. To withdraw Google’s side too, remove obop at myaccount.google.com/permissions.",
      ),
      t("Disconnect"),
      t("Keep it"),
    );
    if (!ok) return;
    setBusy("disconnect");
    const r = await d.run("connection.revoke", { id: settings.connection });
    setBusy("");
    if (fail(r)) return;
    await d.store.load("connection");
    reload();
    setMessage({ tone: "success", text: t("Disconnected. The sign-in was removed from obop.") });
  }

  return (
    <div className="cx-page">
      <div className="cx-head">
        <h2>{t("Google Drive")}</h2>
        <Badge tone={tone(settings.state)}>{label(settings.state)}</Badge>
      </div>
      <p className="hint">
        {t(
          "Browse your Google Drive from the Documents app, add a file, such as a contract written as a Google Doc, as a document, and make folders, Google Docs and uploads in your workspace drive. obop never changes or deletes what is already in Drive.",
        )}
      </p>
      {settings.source === "env" && (
        <Notice tone="info" title={t("Set in the server’s environment")}>
          {t(
            "A Drive is configured with environment variables, which take precedence over this page. Remove OBOP_GDRIVE_BROWSE_REFRESH_TOKEN to use the settings here.",
          )}
        </Notice>
      )}
      {message && <Notice tone={message.tone}>{message.text}</Notice>}
      {settings.state === "connected" && (
        <Notice
          tone="success"
          title={t("Connected as {account}", { account: settings.account || "?" })}
        >
          {t("People in the group {group} can open “From Google Drive” in Documents.", {
            group: settings.browse_group,
          })}
        </Notice>
      )}
      {settings.state === "connected" && !settings.write_granted && (
        <Notice tone="warning" title={t("Creating files is not allowed yet")}>
          {t(
            "The last sign-in only let obop read. Press “Sign in again” and allow creating files.",
          )}
        </Notice>
      )}
      {settings.state === "connected" && settings.workspace_id && settings.write_granted && (
        <Notice tone="success">
          {t(
            "Working in {drive}. People in the group {group} can create folders, Google Docs and uploads there.",
            {
              drive: settings.workspace_name || settings.workspace_id,
              group: settings.write_group,
            },
          )}
        </Notice>
      )}
      {settings.state === "error" && (
        <Notice tone="danger" title={t("The sign-in did not work")}>
          {settings.last_error}
        </Notice>
      )}

      <section aria-label={t("Set up in Google Cloud")}>
        <h3>{t("1. Register obop with Google")}</h3>
        <ol className="hint">
          <li>{t("In Google Cloud Console, create a project and enable the Google Drive API.")}</li>
          <li>
            {t(
              "Configure the OAuth consent screen and publish it. A project left in testing makes Google end the sign-in after 7 days.",
            )}
          </li>
          <li>{t("Create an OAuth client of type “Web application”.")}</li>
          <li>{t("Add this address as an authorised redirect URI:")}</li>
        </ol>
        {redirect ? (
          <div className="cx-secret">
            <pre className="cx-url" style={{ flex: "1 1 20rem" }}>
              {redirect}
            </pre>
            <Button
              onClick={() => {
                void navigator.clipboard?.writeText(redirect);
                d.notify(t("Copied"));
              }}
            >
              {t("Copy")}
            </Button>
          </div>
        ) : (
          <Notice tone="warning" title={t("The public address is not set")}>
            {t(
              "The server does not know its own address, so it cannot start a sign-in. Set OBOP_PUBLIC_URL (for example https://obop.example.com) and restart; the address to register is then that address followed by /oauth/callback.",
            )}
          </Notice>
        )}
      </section>

      <section aria-label={t("The OAuth client")}>
        <h3>{t("2. Enter the client")}</h3>
        <div className="wform">
          <Input
            label={t("OAuth client id")}
            value={id}
            autoComplete="off"
            onChange={(e) => setClientId(e.target.value)}
          />
          <Input
            label={t("OAuth client secret")}
            type="password"
            autoComplete="off"
            value={secret}
            placeholder={settings.secret_set ? "••••••••••••" : ""}
            hint={t(
              "Write-only: it is stored encrypted and never shown again. Leave empty to keep the stored one.",
            )}
            onChange={(e) => setSecret(e.target.value)}
          />
          <Input
            label={t("Who may browse")}
            value={browseGroup}
            autoComplete="off"
            hint={t(
              "A permission group, for example platform.admin or documents.user. One Drive is connected for the whole instance, so keep this narrow.",
            )}
            onChange={(e) => setGroup(e.target.value)}
          />
          <Input
            label={t("Who may create files")}
            value={writeGroupValue}
            autoComplete="off"
            hint={t(
              "A permission group. Creating folders, Google Docs and uploads needs this group and a sign-in that allowed it.",
            )}
            onChange={(e) => setWriteGroup(e.target.value)}
          />
          <div className="actionrow">
            <Button
              variant="primary"
              loading={busy === "save"}
              disabled={!id.trim() || (!secret && !settings.secret_set)}
              onClick={() => void save()}
            >
              {t("Save")}
            </Button>
          </div>
        </div>
      </section>

      <section aria-label={t("Sign in with Google")}>
        <h3>{t("3. Sign in with Google")}</h3>
        <div className="actionrow">
          <Button
            variant="primary"
            loading={busy === "connect"}
            disabled={!configured || !redirect}
            onClick={() => void connect()}
          >
            {settings.state === "connected" || settings.state === "error"
              ? t("Sign in again")
              : t("Connect Google Drive")}
          </Button>
          {settings.connection &&
            (settings.state === "connected" || settings.state === "error") && (
              <Button
                variant="danger"
                loading={busy === "disconnect"}
                onClick={() => void disconnect()}
              >
                {t("Disconnect")}
              </Button>
            )}
        </div>
        {!configured && <p className="hint">{t("Save the client first.")}</p>}
      </section>

      {settings.state === "connected" && settings.source === "admin" && (
        <section aria-label={t("Workspace drive")}>
          <h3>{t("4. Choose the workspace drive")}</h3>
          <p className="hint">
            {t(
              "obop works in this one drive. Browsing, search and imports stay inside it, and new folders, documents and uploads are made there.",
            )}
          </p>
          {drivesError && <Notice tone="danger">{drivesError}</Notice>}
          <div className="wform">
            <Select
              label={t("Workspace drive")}
              value={workspaceId}
              disabled={!drives && !drivesError}
              onChange={(e) => setWorkspace(e.target.value)}
            >
              <option value="">
                {t("None chosen (the whole Drive can be browsed, nothing can be created)")}
              </option>
              <option value="root">{t("My Drive")}</option>
              {(drives ?? []).map((x) => (
                <option key={x.id} value={x.id}>
                  {x.name}
                </option>
              ))}
              {workspaceId &&
                workspaceId !== "root" &&
                !(drives ?? []).some((x) => x.id === workspaceId) && (
                  <option value={workspaceId}>{settings.workspace_name || workspaceId}</option>
                )}
            </Select>
            <p className="hint">
              {t(
                "To make a new shared drive, create it in Google Drive (New, Shared drive) and reload this list.",
              )}{" "}
              <a
                href="https://drive.google.com/drive/shared-drives"
                target="_blank"
                rel="noreferrer noopener"
              >
                {t("Open Google Drive")}
              </a>
            </p>
            <div className="actionrow">
              <Button
                variant="primary"
                loading={busy === "workspace"}
                disabled={workspaceId === settings.workspace_id || !policy}
                onClick={() => void saveWorkspace()}
              >
                {t("Save workspace")}
              </Button>
              <Button onClick={() => setDrivesAgain((n) => n + 1)}>{t("Reload the list")}</Button>
            </div>
          </div>
        </section>
      )}
    </div>
  );
}
