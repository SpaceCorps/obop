import { useCallback, useEffect, useState } from "react";
import { call } from "./api.ts";

type Pkg = { id: string; name: string; version: string; description: string; source: string };

async function rows(token: string, id: string): Promise<Pkg[]> {
  const r = await call(token, id);
  return r.kind === "ok" ? (r.value as Pkg[]) : [];
}

export function Modules({ token, onChange }: { token: string; onChange: () => void }) {
  const [installed, setInstalled] = useState<Pkg[]>([]);
  const [market, setMarket] = useState<Pkg[]>([]);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setInstalled(await rows(token, "module.list"));
    setMarket(await rows(token, "marketplace_listing.list"));
  }, [token]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function act(id: string, body: object, done: string) {
    const r = await call(token, id, body);
    setNotice(
      r.kind === "ok" ? done : r.kind === "error" ? r.message : "Waiting for a human to approve",
    );
    await refresh();
    onChange();
  }

  const have = new Set(installed.map((p) => p.id));

  return (
    <>
      <h2>Modules</h2>
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      <h3>Installed</h3>
      <table>
        <thead>
          <tr>
            <th>Module</th>
            <th>Version</th>
            <th>Source</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {installed.map((p) => (
            <tr key={p.id}>
              <td>
                <strong>{p.name}</strong>
                <div className="hint">{p.description}</div>
              </td>
              <td>{p.version}</td>
              <td>{p.source}</td>
              <td className="actions">
                {p.source !== "builtin" && (
                  <button
                    onClick={() => act("module.uninstall", { id: p.id }, `Removed ${p.name}`)}
                  >
                    Uninstall
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <h3>Marketplace</h3>
      <table>
        <tbody>
          {market.map((p) => (
            <tr key={p.id}>
              <td>
                <strong>{p.name}</strong>
                <div className="hint">{p.description}</div>
              </td>
              <td>{p.version}</td>
              <td className="actions">
                <button
                  disabled={have.has(p.id)}
                  onClick={() =>
                    act("marketplace_listing.install", { id: p.id }, `Installed ${p.name}`)
                  }
                >
                  {have.has(p.id) ? "Installed" : "Install"}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <h3>Upload your own</h3>
      <p className="hint">
        An obop.toml file: models, fields with data classes, permission groups and actions.
      </p>
      <input
        type="file"
        accept=".toml"
        onChange={async (e) => {
          const file = e.target.files?.[0];
          if (file)
            await act("module.install", { manifest: await file.text() }, `Installed ${file.name}`);
          e.target.value = "";
        }}
      />
    </>
  );
}
