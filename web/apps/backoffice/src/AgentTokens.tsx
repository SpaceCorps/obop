import { useCallback, useEffect, useId, useRef, useState } from "react";
import { Button, Input, Notice, Table, fmt, t } from "@obop/ui";
import { GroupPicker } from "./People.tsx";
import { account, reason, type Notify } from "./SecurityAccount.tsx";
import { copyText } from "./signLinks.ts";

/* Customize > Security: the agent tokens of the person who is signed in. A token lets a program or
   an AI agent act for them with only the permission groups they pick. Everything goes through the
   account routes under /api/auth: the list with the groups the person may hand over, create, and
   revoke.

   A new token is a secret that the server shows once. It is held in the state of this section while
   it is on the screen and nowhere else: not in storage, the address, the console or a notification. */

/** The longest label the server takes. */
const LABEL_MAX = 80;
/** With more groups than this to choose from, the list gets a filter. */
const FILTER_ABOVE = 12;

type TokenRow = {
  /** 64 hex characters. */
  id: string;
  label: string;
  scopes: string[];
  /** Unix seconds. */
  created_at: number;
  /** Unix seconds when the token ends by itself; null when it has none (N18). */
  expires_at?: number | null;
};

type Listing = {
  /** The person's own agent tokens that still work. */
  tokens: TokenRow[];
  /** The groups the person may put on a token, sorted. */
  delegable: string[];
};

/** What a token is called on the screen: its label, or the start of its id when it has none. */
const nameOf = (row: TokenRow) => row.label.trim() || row.id.slice(0, 8);

/** The agent tokens you issued, with a way to revoke each and to create one. */
export function AgentTokensSection({ token, notify }: { token: string; notify: Notify }) {
  const id = useId();
  const [data, setData] = useState<Listing | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [absent, setAbsent] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [label, setLabel] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [secret, setSecret] = useState<string | null>(null);
  const shown = useRef<HTMLDivElement>(null);
  const form = useRef<HTMLFormElement>(null);
  const wasShown = useRef(false);

  const load = useCallback(async () => {
    const r = await account<Listing>(token, "GET", "/agent-tokens");
    if (!r.ok) {
      // A server without the route has no agent tokens to manage: nothing to show. Any other
      // refusal is shown as one, with what was read before kept (a refused read is not "no tokens").
      if (r.status === 404 || r.status === 501) setAbsent(true);
      else setProblem(reason(r) || t("The agent tokens could not be loaded. Try again."));
      return;
    }
    setProblem(null);
    setData({ tokens: r.value.tokens ?? [], delegable: r.value.delegable ?? [] });
  }, [token]);
  useEffect(() => void load(), [load]);

  // The keyboard follows the token: to Copy when it appears, back to the form when it is put away.
  useEffect(() => {
    if (secret !== null) {
      wasShown.current = true;
      shown.current?.querySelector<HTMLButtonElement>("[data-copy]")?.focus();
    } else if (wasShown.current) {
      wasShown.current = false;
      form.current?.querySelector<HTMLInputElement>("input")?.focus();
    }
  }, [secret]);

  if (absent || (data === null && problem === null)) return null;
  const tokens = data?.tokens ?? [];
  const delegable = data?.delegable ?? [];
  // In the server's order, and never a group that is no longer on offer.
  const chosen = delegable.filter((g) => picked.includes(g));

  async function create() {
    const name = label.trim();
    if (!name || chosen.length === 0) return;
    setCreating(true);
    setError(null);
    const r = await account<{ token: string }>(token, "POST", "/agent-token", {
      scopes: chosen,
      label: name,
    });
    setCreating(false);
    if (r.ok && typeof r.value.token === "string" && r.value.token !== "") {
      setSecret(r.value.token);
      setLabel("");
      setPicked([]);
      void load();
    } else if (r.ok) setError(t("Could not create the agent token."));
    else
      setError(
        reason(r) || (r.status === 400 ? r.message : "") || t("Could not create the agent token."),
      );
  }

  async function revoke(row: TokenRow) {
    setBusy(row.id);
    const r = await account<{ revoked: boolean }>(
      token,
      "POST",
      `/agent-tokens/${encodeURIComponent(row.id)}/revoke`,
      {},
    );
    setBusy(null);
    if (r.ok) notify(t("Agent token {name} revoked.", { name: nameOf(row) }));
    else notify(reason(r) || t("Could not revoke the agent token."), "error");
    // Also after a refusal: a token that was already gone leaves the list.
    void load();
  }

  async function copy() {
    if (secret === null) return;
    const field = shown.current?.querySelector<HTMLInputElement>("input") ?? null;
    if (await copyText(secret, field)) notify(t("Copied"));
    else notify(t("Could not copy. Select the token and copy it yourself."), "error");
  }

  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="overline">
        {t("Agent tokens")}
      </h3>
      <p className="hint">
        {t(
          "An agent token lets a program or an AI agent act for you with only the permission groups you pick. It is shown once.",
        )}
      </p>
      {problem !== null && (
        <Notice tone="warning" title={t("Agent tokens")}>
          {problem}{" "}
          <Button size="sm" onClick={() => void load()}>
            {t("Try again")}
          </Button>
        </Notice>
      )}
      {data !== null && tokens.length === 0 && problem === null && (
        <p className="hint">{t("No agent tokens.")}</p>
      )}
      {tokens.length > 0 && (
        <Table caption={t("Agent tokens")} dense stack>
          <thead>
            <tr>
              <th scope="col">{t("Label")}</th>
              <th scope="col">{t("Can act with")}</th>
              <th scope="col">{t("Created")}</th>
              <th scope="col">{t("Ends")}</th>
              <th scope="col">
                <span className="sr-only">{t("Actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {tokens.map((row) => (
              <tr key={row.id}>
                <td className="cell-main">
                  <strong>{nameOf(row)}</strong>
                </td>
                <td data-label={t("Can act with")}>{row.scopes.join(", ")}</td>
                <td data-label={t("Created")}>
                  {row.created_at ? fmt.dateTime(row.created_at * 1000) : ""}
                </td>
                <td data-label={t("Ends")}>
                  {row.expires_at ? fmt.dateTime(row.expires_at * 1000) : t("Until revoked")}
                </td>
                <td className="cell-actions">
                  <Button
                    variant="danger"
                    size="sm"
                    loading={busy === row.id}
                    aria-label={t("Revoke {name}", { name: nameOf(row) })}
                    onClick={() => void revoke(row)}
                  >
                    {t("Revoke")}
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}
      {secret !== null && (
        // Outside every form, so that no browser keeps the value among its form entries.
        <div className="stack" ref={shown}>
          <Notice tone="warning" title={t("Shown once")}>
            {t("This is the only time the token is shown. Store it like a password.")}
          </Notice>
          <Input
            label={t("Agent token")}
            className="agent-token-field"
            value={secret}
            readOnly
            spellCheck={false}
            autoComplete="off"
            data-testid="agent-token"
            onFocus={(e) => e.currentTarget.select()}
          />
          <div className="actionrow">
            <Button data-copy onClick={() => void copy()}>
              {t("Copy")}
            </Button>
            <Button variant="primary" onClick={() => setSecret(null)}>
              {t("Done")}
            </Button>
          </div>
        </div>
      )}
      {data !== null && secret === null && delegable.length === 0 && (
        <p className="hint">{t("You have no permission groups to give to a token.")}</p>
      )}
      {data !== null && secret === null && delegable.length > 0 && (
        <form
          ref={form}
          className="stack"
          aria-label={t("Create agent token")}
          onSubmit={(e) => {
            e.preventDefault();
            void create();
          }}
          onKeyDown={(e) => {
            // Enter in the filter or on a group does not create a token: only the label and the
            // button do.
            if (e.key === "Enter" && (e.target as HTMLElement).closest(".group-picker"))
              e.preventDefault();
          }}
        >
          <Input
            label={t("Label")}
            hint={t("The program or agent that will use the token. Up to {n} characters.", {
              n: LABEL_MAX,
            })}
            name="agent_token_label"
            value={label}
            maxLength={LABEL_MAX}
            onChange={(e) => setLabel(e.target.value)}
            autoComplete="off"
            required
          />
          <GroupPicker
            groups={delegable}
            value={chosen}
            onChange={setPicked}
            filterAbove={FILTER_ABOVE}
          />
          {error && (
            <p className="hint" role="alert">
              {error}
            </p>
          )}
          <div className="actionrow">
            <Button
              type="submit"
              variant="primary"
              loading={creating}
              disabled={!label.trim() || chosen.length === 0}
            >
              {t("Create agent token")}
            </Button>
          </div>
        </form>
      )}
    </section>
  );
}
