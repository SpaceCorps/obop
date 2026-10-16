import { useCallback, useEffect, useMemo, useState } from "react";
import { approve, call, commands, type Command, type Reply, type Row } from "./api.ts";

const TOKENS = ["dev-admin", "dev-sales", "dev-agent"];

export function App() {
  const [token, setToken] = useState("dev-admin");
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
    commands(token).then((c) => {
      setCmds(c);
      const first = c.find((x) => x.id.endsWith(".list"))?.id.split(".")[0] ?? null;
      setModel((m) => (m && c.some((x) => x.id === `${m}.list`) ? m : first));
    });
  }, [token]);

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

  const columns = rows[0] ? Object.keys(rows[0]).filter((k) => k !== "id") : [];

  return (
    <div className="shell">
      <aside>
        <h1>obop</h1>
        <label>
          Signed in as
          <select value={token} onChange={(e) => setToken(e.target.value)}>
            {TOKENS.map((t) => (
              <option key={t}>{t}</option>
            ))}
          </select>
        </label>
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
