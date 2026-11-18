import { useCallback, useEffect, useId, useRef, useState } from "react";
import {
  Badge,
  Button,
  Input,
  Notice,
  Segmented,
  Select,
  Switch,
  Table,
  Textarea,
  fmt,
  t,
} from "@obop/ui";
import { call, search, type Row } from "../api.ts";
import "./mail.css";
import { href } from "../route.ts";
import { serverMessage } from "../serverMessage.ts";

/* Customize > Mail (docs/spec/mail.md): the mail servers (an SMTP relay, or a provider reached through
   a Connectors connection), a test message, and the deliveries log. Everything goes through the
   commands of the `mail` module: mail_server.create|update|test_send, outbound_mail.search|retry|cancel.
   The password is write-only: the server seals it and never returns it. The log never shows a body. */

type Notify = (text: string, tone?: "ok" | "error" | "info") => void;

/** The providers of the Connectors catalogue that send mail (engine `mail`). */
const MAIL_PROVIDERS = new Set([
  "postmark",
  "sendgrid",
  "mailgun",
  "microsoft_graph_mail",
  "gmail_send",
]);

type Server = Row & {
  name?: string;
  kind?: "smtp" | "provider";
  enabled?: boolean;
  is_default?: boolean;
  kinds?: string;
  host?: string;
  port?: number | null;
  security?: string;
  username?: string;
  password_set?: boolean;
  connection?: string;
  from_address?: string;
  from_name?: string;
  reply_to?: string;
  footer?: string;
  last_test_at?: string | null;
  last_test_result?: string;
};

type Draft = {
  id?: string;
  name: string;
  kind: "smtp" | "provider";
  enabled: boolean;
  is_default: boolean;
  kinds: string;
  host: string;
  port: string;
  security: string;
  username: string;
  password: string;
  password_set: boolean;
  connection: string;
  from_address: string;
  from_name: string;
  reply_to: string;
  footer: string;
};

const EMPTY: Draft = {
  name: "",
  kind: "smtp",
  enabled: true,
  is_default: false,
  kinds: "",
  host: "",
  port: "587",
  security: "starttls",
  username: "",
  password: "",
  password_set: false,
  connection: "",
  from_address: "",
  from_name: "",
  reply_to: "",
  footer: "",
};

const s = (v: unknown) => (typeof v === "string" ? v : v == null ? "" : String(v));

function draftOf(r: Server): Draft {
  return {
    id: r.id,
    name: s(r.name),
    kind: r.kind === "provider" ? "provider" : "smtp",
    enabled: r.enabled === true,
    is_default: r.is_default === true,
    kinds: s(r.kinds),
    host: s(r.host),
    port: s(r.port),
    security: s(r.security) || "starttls",
    username: s(r.username),
    password: "",
    password_set: r.password_set === true,
    connection: s(r.connection),
    from_address: s(r.from_address),
    from_name: s(r.from_name),
    reply_to: s(r.reply_to),
    footer: s(r.footer),
  };
}

const message = (r: { kind: string; message?: string }, fallback: string) =>
  r.kind === "error"
    ? serverMessage(r.message) || fallback
    : r.kind === "approval"
      ? t("Sent for approval.")
      : fallback;

const statusTone = (st: string) =>
  st === "sent"
    ? "success"
    : st === "failed"
      ? "danger"
      : st === "queued" || st === "sending"
        ? "warning"
        : ("neutral" as const);

const statusLabel = (st: string) =>
  ({
    queued: t("Queued"),
    sending: t("Sending"),
    sent: t("Sent"),
    failed: t("Failed"),
    cancelled: t("Cancelled"),
  })[st] ?? st;

/** The form that adds or changes one mail server. */
function ServerForm({
  token,
  draft,
  connections,
  onDone,
  onCancel,
}: {
  token: string;
  draft: Draft;
  connections: Row[];
  onDone: (text: string) => void;
  onCancel: () => void;
}) {
  const [d, setD] = useState<Draft>(draft);
  const form = useRef<HTMLFormElement>(null);
  // The button that opened the form is gone or below it: the keyboard starts at the first field.
  useEffect(() => form.current?.querySelector<HTMLInputElement>("input")?.focus(), []);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => setD((x) => ({ ...x, [k]: v }));
  const mailConnections = connections.filter((c) => MAIL_PROVIDERS.has(s(c.provider)));

  async function save() {
    setSaving(true);
    setError(null);
    const body: Record<string, unknown> = {
      name: d.name.trim(),
      kind: d.kind,
      enabled: d.enabled,
      is_default: d.is_default,
      kinds: d.kinds.trim(),
      from_address: d.from_address.trim(),
      from_name: d.from_name.trim(),
      reply_to: d.reply_to.trim(),
      footer: d.footer,
    };
    if (d.kind === "smtp") {
      Object.assign(body, {
        host: d.host.trim(),
        port: d.port.trim() ? Number(d.port.trim()) : null,
        security: d.security,
        username: d.username.trim(),
      });
      // Empty keeps the stored password.
      if (d.password) body.password = d.password;
    } else body.connection = d.connection;
    if (d.id) body.id = d.id;
    const r = await call(token, d.id ? "mail_server.update" : "mail_server.create", body);
    setSaving(false);
    if (r.kind === "ok") onDone(d.id ? t("Mail server saved.") : t("Mail server added."));
    else setError(message(r, t("Could not save the mail server.")));
  }

  return (
    <form
      ref={form}
      className="stack mail-form"
      aria-label={d.id ? t("Edit mail server") : t("Add mail server")}
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <div className="two">
        <Input
          label={t("Name")}
          value={d.name}
          required
          autoComplete="off"
          onChange={(e) => set("name", e.target.value)}
        />
        <div className="field">
          <span className="flabel" aria-hidden="true">
            {t("Sends through")}
          </span>
          <Segmented
            label={t("Sends through")}
            value={d.kind}
            onChange={(k) => set("kind", k)}
            options={[
              { value: "smtp", label: t("SMTP relay") },
              { value: "provider", label: t("Provider") },
            ]}
          />
        </div>
      </div>
      {d.kind === "smtp" ? (
        <>
          <div className="two">
            <Input
              label={t("Host")}
              value={d.host}
              required
              placeholder="smtp.example.com"
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => set("host", e.target.value)}
            />
            <div className="two">
              <Select
                label={t("Security")}
                value={d.security}
                onChange={(e) => {
                  const sec = e.target.value;
                  set("security", sec);
                  if (d.port === "587" || d.port === "465" || d.port === "25" || !d.port)
                    set("port", sec === "tls" ? "465" : sec === "none" ? "25" : "587");
                }}
              >
                <option value="starttls">{t("STARTTLS")}</option>
                <option value="tls">{t("TLS")}</option>
                <option value="none">{t("None (no password)")}</option>
              </Select>
              <Input
                label={t("Port")}
                value={d.port}
                inputMode="numeric"
                onChange={(e) => set("port", e.target.value.replace(/\D/g, "").slice(0, 5))}
              />
            </div>
          </div>
          <div className="two">
            <Input
              label={t("User name")}
              value={d.username}
              autoComplete="off"
              hint={t("Empty means the relay takes mail without signing in.")}
              onChange={(e) => set("username", e.target.value)}
            />
            <Input
              label={t("Password")}
              type="password"
              value={d.password}
              autoComplete="new-password"
              placeholder={d.password_set ? t("Stored; type to replace") : ""}
              hint={t("Stored encrypted and never shown again.")}
              onChange={(e) => set("password", e.target.value)}
            />
          </div>
        </>
      ) : (
        <>
          <Select
            label={t("Connection")}
            value={d.connection}
            required
            hint={t(
              "A connection of a mail provider (Postmark, SendGrid, Mailgun, Microsoft 365 or Gmail) made in the Connectors app.",
            )}
            onChange={(e) => set("connection", e.target.value)}
          >
            <option value="">{t("Choose a connection")}</option>
            {mailConnections.map((c) => (
              <option key={c.id} value={c.id}>
                {s(c.name) || c.id}
              </option>
            ))}
          </Select>
          {mailConnections.length === 0 && (
            <p className="hint">
              {t("No mail provider is connected yet.")}{" "}
              <a href={href({ app: "connectors" })}>{t("Open Connectors")}</a>
            </p>
          )}
        </>
      )}
      <div className="two">
        <Input
          label={t("Sender address")}
          type="email"
          value={d.from_address}
          required
          autoComplete="off"
          spellCheck={false}
          onChange={(e) => set("from_address", e.target.value)}
        />
        <Input
          label={t("Sender name")}
          value={d.from_name}
          autoComplete="off"
          onChange={(e) => set("from_name", e.target.value)}
        />
      </div>
      <div className="two">
        <Input
          label={t("Reply-to address")}
          type="email"
          value={d.reply_to}
          autoComplete="off"
          spellCheck={false}
          hint={t("Empty means replies go to the sender address.")}
          onChange={(e) => set("reply_to", e.target.value)}
        />
        <Input
          label={t("Message kinds")}
          value={d.kinds}
          autoComplete="off"
          spellCheck={false}
          placeholder="invoice, password_reset"
          hint={t("Comma separated. Empty means any kind.")}
          onChange={(e) => set("kinds", e.target.value)}
        />
      </div>
      <Textarea
        label={t("Footer")}
        value={d.footer}
        rows={3}
        hint={t("Added under every message, such as your company details.")}
        onChange={(e) => set("footer", e.target.value)}
      />
      <div className="mail-switches">
        <Switch
          label={t("Enabled")}
          checked={d.enabled}
          onChange={(e) => set("enabled", e.target.checked)}
        />
        <Switch
          label={t("Default server")}
          checked={d.is_default}
          onChange={(e) => set("is_default", e.target.checked)}
        />
      </div>
      {error && (
        <p className="hint" role="alert">
          {error}
        </p>
      )}
      <div className="actionrow">
        <Button type="submit" variant="primary" loading={saving}>
          {d.id ? t("Save") : t("Add mail server")}
        </Button>
        <Button onClick={onCancel}>{t("Cancel")}</Button>
      </div>
    </form>
  );
}

/** Send a test message through one server. */
function TestSend({
  token,
  server,
  onDone,
}: {
  token: string;
  server: Server;
  onDone: () => void;
}) {
  const [to, setTo] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  async function send() {
    setBusy(true);
    setResult(null);
    const r = await call(token, "mail_server.test_send", {
      id: server.id,
      test_recipient: to.trim(),
    });
    setBusy(false);
    if (r.kind === "ok")
      setResult({ ok: true, text: t("The test message was sent to {to}.", { to: to.trim() }) });
    else setResult({ ok: false, text: message(r, t("The test message was not sent.")) });
    onDone();
  }
  return (
    <form
      className="mail-test"
      aria-label={t("Send a test message")}
      onSubmit={(e) => {
        e.preventDefault();
        void send();
      }}
    >
      <Input
        label={t("Send a test message to")}
        type="email"
        value={to}
        required
        autoComplete="email"
        spellCheck={false}
        onChange={(e) => setTo(e.target.value)}
      />
      <Button type="submit" loading={busy} disabled={!to.trim()}>
        {t("Send test")}
      </Button>
      {result && (
        <p className={result.ok ? "hint" : "hint mail-error"} role="status">
          {result.text}
        </p>
      )}
    </form>
  );
}

function ServersSection({
  token,
  notify,
  onTested,
}: {
  token: string;
  notify: Notify;
  /** A test message was sent or refused: it is a row of the deliveries log now. */
  onTested: () => void;
}) {
  const id = useId();
  const section = useRef<HTMLElement>(null);
  const [rows, setRows] = useState<Server[] | null>(null);
  const [connections, setConnections] = useState<Row[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [editing, setEditing] = useState<Draft | null>(null);
  const [testing, setTesting] = useState<string | null>(null);
  // Which button opened the form ("add" or a server's id), to hand the keyboard back to it.
  const opener = useRef<string | null>(null);
  const open = (draft: Draft, from: string) => {
    opener.current = from;
    setEditing(draft);
  };
  const close = () => {
    setEditing(null);
    const from = opener.current;
    opener.current = null;
    // After the render that brings the button back.
    requestAnimationFrame(() =>
      section.current?.querySelector<HTMLElement>(`[data-mail-opener="${from ?? "add"}"]`)?.focus(),
    );
  };

  const load = useCallback(async () => {
    const r = await search(token, "mail_server", {
      order: [{ field: "name", dir: "asc" }],
      limit: 200,
    });
    if ("error" in r)
      setProblem(serverMessage(r.error) || t("The mail servers could not be loaded."));
    else {
      setProblem(null);
      setRows(r.rows as Server[]);
    }
    const c = await search(token, "connection", { limit: 500 });
    if (!("error" in c)) setConnections(c.rows);
  }, [token]);
  useEffect(() => void load(), [load]);

  return (
    <section ref={section} className="stack" aria-labelledby={`${id}-h`}>
      <div className="mail-head">
        <h3 id={`${id}-h`} className="overline">
          {t("Mail servers")}
        </h3>
        {editing === null && (
          <Button
            size="sm"
            variant="primary"
            data-mail-opener="add"
            onClick={() => open({ ...EMPTY }, "add")}
          >
            {t("Add mail server")}
          </Button>
        )}
      </div>
      <p className="hint">
        {t(
          "Mail leaves through your own SMTP relay or a provider you connect. Each server has its sender and can serve some message kinds; one can be the default.",
        )}
      </p>
      {problem && (
        <Notice tone="warning" title={t("Mail servers")}>
          {problem}{" "}
          <Button size="sm" onClick={() => void load()}>
            {t("Try again")}
          </Button>
        </Notice>
      )}
      {editing && (
        <ServerForm
          key={editing.id ?? "new"}
          token={token}
          draft={editing}
          connections={connections}
          onCancel={close}
          onDone={(text) => {
            close();
            notify(text);
            void load();
          }}
        />
      )}
      {rows !== null && rows.length === 0 && editing === null && (
        <Notice tone="info" title={t("No mail server yet")}>
          {t("Nothing can be sent until you add one.")}
        </Notice>
      )}
      {rows !== null && rows.length > 0 && (
        <ul className="mail-servers">
          {rows.map((r) => (
            <li key={r.id} className="mail-server">
              <div className="mail-server-top">
                <strong>{s(r.name)}</strong>
                <span className="mail-badges">
                  {r.is_default && <Badge tone="info">{t("Default")}</Badge>}
                  <Badge tone={r.enabled ? "success" : "neutral"}>
                    {r.enabled ? t("Enabled") : t("Off")}
                  </Badge>
                </span>
              </div>
              <div className="hint">
                {r.kind === "provider"
                  ? t("Provider: {name}", {
                      name:
                        s(connections.find((c) => c.id === r.connection)?.name) ||
                        t("a connection"),
                    })
                  : t("SMTP {host}:{port}", { host: s(r.host), port: s(r.port) || "587" })}
                {" · "}
                {s(r.from_name) ? `${s(r.from_name)} <${s(r.from_address)}>` : s(r.from_address)}
              </div>
              {s(r.kinds) && (
                <div className="hint">{t("Kinds: {kinds}", { kinds: s(r.kinds) })}</div>
              )}
              {s(r.last_test_result) && (
                <div
                  className={s(r.last_test_result).startsWith("sent") ? "hint" : "hint mail-error"}
                >
                  {t("Last test {when}: {result}", {
                    when: r.last_test_at ? fmt.dateTime(Date.parse(s(r.last_test_at))) : "",
                    result: s(r.last_test_result),
                  })}
                </div>
              )}
              <div className="actionrow">
                <Button size="sm" data-mail-opener={r.id} onClick={() => open(draftOf(r), r.id)}>
                  {t("Edit")}
                </Button>
                <Button
                  size="sm"
                  aria-expanded={testing === r.id}
                  onClick={() => setTesting(testing === r.id ? null : r.id)}
                >
                  {t("Send test")}
                </Button>
              </div>
              {testing === r.id && (
                <TestSend
                  token={token}
                  server={r}
                  onDone={() => {
                    void load();
                    onTested();
                  }}
                />
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function DeliveriesSection({
  token,
  notify,
  stamp,
}: {
  token: string;
  notify: Notify;
  /** Changes when something else on the tab added a row (a test message). */
  stamp: number;
}) {
  const id = useId();
  const [rows, setRows] = useState<Row[] | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const load = useCallback(async () => {
    const r = await search(token, "outbound_mail", {
      // queued_at has whole seconds; the ids (UUIDv7) keep the order of a burst within one.
      order: [
        { field: "queued_at", dir: "desc" },
        { field: "id", dir: "desc" },
      ],
      limit: 50,
    });
    if ("error" in r)
      setProblem(serverMessage(r.error) || t("The deliveries could not be loaded."));
    else {
      setProblem(null);
      setRows(r.rows);
    }
  }, [token]);
  useEffect(() => void load(), [load, stamp]);

  async function act(row: Row, command: "retry" | "cancel") {
    setBusy(row.id);
    const r = await call(token, `outbound_mail.${command}`, { id: row.id });
    setBusy(null);
    if (r.kind === "ok") notify(command === "retry" ? t("Queued again.") : t("Cancelled."));
    else notify(message(r, t("That did not work.")), "error");
    void load();
  }

  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <div className="mail-head">
        <h3 id={`${id}-h`} className="overline">
          {t("Deliveries")}
        </h3>
        <Button size="sm" onClick={() => void load()}>
          {t("Refresh")}
        </Button>
      </div>
      <p className="hint">
        {t("The last 50 messages. The text of a message is never shown here.")}{" "}
        <a href={href({ app: "mail", model: "outbound_mail" })}>{t("All deliveries")}</a>
      </p>
      {problem && (
        <Notice tone="warning" title={t("Deliveries")}>
          {problem}
        </Notice>
      )}
      {rows !== null && rows.length === 0 && <p className="hint">{t("No messages yet.")}</p>}
      {rows !== null && rows.length > 0 && (
        <Table caption={t("Deliveries")} dense stack>
          <thead>
            <tr>
              <th scope="col">{t("Queued")}</th>
              <th scope="col">{t("Recipient")}</th>
              <th scope="col">{t("Subject")}</th>
              <th scope="col">{t("Status")}</th>
              <th scope="col">{t("Attempts")}</th>
              <th scope="col">
                <span className="sr-only">{t("Actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => {
              const st = s(r.status);
              return (
                <tr key={r.id}>
                  <td data-label={t("Queued")}>
                    {r.queued_at ? fmt.dateTime(Date.parse(s(r.queued_at))) : ""}
                  </td>
                  <td className="cell-main mail-to" data-label={t("Recipient")}>
                    {s(r.to)}
                  </td>
                  <td data-label={t("Subject")}>
                    {s(r.subject)}
                    {s(r.last_error) && st !== "sent" && (
                      <span className="hint block mail-error">{s(r.last_error)}</span>
                    )}
                  </td>
                  <td className="mail-status" data-label={t("Status")}>
                    <Badge tone={statusTone(st)}>{statusLabel(st)}</Badge>
                  </td>
                  <td data-label={t("Attempts")}>{s(r.attempts)}</td>
                  <td className="cell-actions">
                    {st === "failed" && s(r.kind) !== "test" && (
                      <Button
                        size="sm"
                        loading={busy === r.id}
                        onClick={() => void act(r, "retry")}
                      >
                        {t("Send again")}
                      </Button>
                    )}
                    {st === "queued" && (
                      <Button
                        size="sm"
                        variant="danger"
                        loading={busy === r.id}
                        onClick={() => void act(r, "cancel")}
                      >
                        {t("Cancel")}
                      </Button>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </Table>
      )}
    </section>
  );
}

/** The Mail tab of Customize. */
export function MailTab({ token, notify }: { token: string; notify: Notify }) {
  const [stamp, setStamp] = useState(0);
  return (
    <div className="stack mail-tab">
      <ServersSection token={token} notify={notify} onTested={() => setStamp((n) => n + 1)} />
      <DeliveriesSection token={token} notify={notify} stamp={stamp} />
      <section className="stack">
        <h3 className="overline">{t("Templates")}</h3>
        <p className="hint">
          {t(
            "A template holds the subject and text of one message kind in one language. A placeholder such as {example} is filled in when the message is queued.",
            { example: "{{name}}" },
          )}{" "}
          <a href={href({ app: "mail", model: "mail_template" })}>{t("Edit templates")}</a>
        </p>
      </section>
    </div>
  );
}
