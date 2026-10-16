import { useCallback, useEffect, useMemo, useState } from "react";
import { approve, call, commands, login, type Command, type Reply, type Row } from "./api.ts";

function stored(): string {
  try {
    return sessionStorage.getItem("obop-token") ?? "";
  } catch {
    return "";
  }
}

function remember(token: string) {
  try {
    if (token) sessionStorage.setItem("obop-token", token);
    else sessionStorage.removeItem("obop-token");
  } catch {
    /* storage unavailable: the session lasts until reload */
  }
}

function SignIn({ onToken }: { onToken: (t: string) => void }) {
  const [error, setError] = useState<string | null>(null);
  return (
    <form
      className="signin"
      onSubmit={async (e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget);
        const t = await login(String(f.get("username")), String(f.get("password")));
        if (t) onToken(t);
        else setError("Invalid username or password");
      }}
    >
      <h1>obop</h1>
      <label>
        Username
        <input name="username" autoComplete="username" required autoFocus />
      </label>
      <label>
        Password
        <input name="password" type="password" autoComplete="current-password" required />
      </label>
      {error && <p role="alert">{error}</p>}
      <button type="submit">Sign in</button>
    </form>
  );
}

export function App() {
  const [token, setTokenState] = useState(stored);
  const setToken = useCallback((t: string) => {
    remember(t);
    setTokenState(t);
  }, []);
  const [cmds, setCmds] = useState<Command[]>([]);
  const [model, setModel] = useState<string | null>(null);
  const [rows, setRows] = useState<Row[]>([]);
  const [denied, setDenied] = useState<string[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [plan, setPlan] = useState<string | null>(null);

  const models = useMemo(
    () => cmds.filter((c) => c.id.endsWith(".list")).map((c) => c.id.split(".")[0]!),
    [cmds],
  );
  const custom = useMemo(
    () =>
      cmds.filter(
        (c) => model && c.id.startsWith(`${model}.`) && !/\.(get|list|create|update)$/.test(c.id),
      ),
    [cmds, model],
  );

  useEffect(() => {
    if (!token) return;
    commands(token).then((c) => {
      if (c === null) return setToken("");
      setCmds(c);
      const first = c.find((x) => x.id.endsWith(".list"))?.id.split(".")[0] ?? null;
      setModel((m) => (m && c.some((x) => x.id === `${m}.list`) ? m : first));
    });
  }, [token, setToken]);

  const load = useCallback(async () => {
    if (!model) return setRows([]);
    const r = await call(token, `${model}.list`);
    if (r.kind === "ok") {
      setRows(r.value as Row[]);
      setDenied(r.denied);
    }
  }, [token, model]);

  useEffect(() => {
    void load();
  }, [load]);

  async function run(id: string, rowId: string) {
    const r: Reply = await call(token, id, { id: rowId });
    if (r.kind === "approval") {
      setPlan(r.planId);
      setNotice(`${id} needs approval. Plan ${r.planId.slice(0, 8)} is waiting for a human.`);
    } else if (r.kind === "error") setNotice(`${id}: ${r.message}`);
    else {
      setNotice(`${id} done`);
      await load();
    }
  }

  async function approvePlan() {
    if (!plan) return;
    const r = await approve(token, plan);
    setNotice(
      r.kind === "ok"
        ? "Approved and executed"
        : "Only a human with the right group can approve this plan",
    );
    if (r.kind === "ok") setPlan(null);
    await load();
  }

  if (!token) return <SignIn onToken={setToken} />;

  const columns = rows[0] ? Object.keys(rows[0]).filter((k) => k !== "id") : [];

  return (
    <div className="shell">
      <aside>
        <h1>obop</h1>
        <button className="signout" onClick={() => setToken("")}>
          Sign out
        </button>
        <nav aria-label="Models">
          {models.map((m) => (
            <button key={m} aria-current={m === model} onClick={() => setModel(m)}>
              {m.replace("_", " ")}
            </button>
          ))}
        </nav>
      </aside>
      <main>
        <h2>{model?.replace("_", " ") ?? "Nothing available"}</h2>
        {denied.length > 0 && (
          <p className="hint">Withheld from this principal: {denied.join(", ")}</p>
        )}
        {notice && (
          <p className="notice" role="status">
            {notice} {plan && <button onClick={approvePlan}>Approve</button>}
          </p>
        )}
        <table>
          <thead>
            <tr>
              {columns.map((c) => (
                <th key={c}>{c}</th>
              ))}
              <th />
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={String(r.id)}>
                {columns.map((c) => (
                  <td key={c}>{String(r[c] ?? "")}</td>
                ))}
                <td className="actions">
                  {custom.map((c) => (
                    <button
                      key={c.id}
                      title={c.description}
                      onClick={() => run(c.id, String(r.id))}
                    >
                      {c.id.split(".")[1]}
                      {c.needs_approval ? " *" : ""}
                    </button>
                  ))}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {rows.length === 0 && <p className="hint">No records.</p>}
        <p className="hint">* agents only propose these; a human approves.</p>
      </main>
    </div>
  );
}
