import { useEffect, useRef, useState } from "react";
import { Button, Dialog, Icon, Input, Notice, Spinner, Table, fmt, t } from "@obop/ui";
import { request } from "./net.ts";
import { serverMessage } from "./serverMessage.ts";
import { formatBytes } from "./blobs.ts";

/* The Drive browser: look through the Google Drive an administrator connected to this instance,
   search it by name, and bring one file into a record. A Google Doc, Sheet or Slides file comes as
   PDF. The server fetches the file and stores it as an upload for the record's file field
   (`POST /api/drive/import`); the person then reviews and saves the new record as for any file. */

type Entry = {
  id: string;
  name: string;
  mime: string;
  folder: boolean;
  modified: string;
  size: number | null;
  web_link: string;
  importable: boolean;
};

type Workspace = { id: string; name: string } | null;

type Created = { id: string; name: string; folder: boolean; web_link: string };

type Imported = {
  id: string;
  filename: string;
  drive: { id: string; name: string; web_link: string; modified: string };
};

/** What the dialog hands back: the values a new record starts with. */
export type DriveDefaults = { title: string; location: string; attachment: string };

const auth = (token: string) => ({ authorization: `Bearer ${token}` });

/**
 * Whether a Drive is connected and this person may browse it, and whether this person could set
 * one up (an administrator of connectors, while none is connected); asked once per token.
 */
export function useDriveStatus(
  token: string,
  wanted: boolean,
): { may: boolean; canSetUp: boolean } {
  const [st, setSt] = useState({ may: false, canSetUp: false });
  useEffect(() => {
    if (!wanted) return;
    let live = true;
    request("/api/drive/status", { headers: auth(token) }, { retry: true })
      .then((r) => (r.ok ? r.json() : null))
      .then(
        (v: { connected?: boolean; may_browse?: boolean; may_configure?: boolean } | null) =>
          live &&
          setSt({
            may: v?.may_browse === true,
            canSetUp: v?.may_configure === true && v.connected !== true,
          }),
      )
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [token, wanted]);
  return st;
}

type Crumb = { id: string; name: string };

export function DriveBrowser({
  token,
  field,
  onClose,
  onPicked,
}: {
  token: string;
  /** The upload field the file goes into, `model.field`. */
  field: string;
  onClose: () => void;
  onPicked: (defaults: DriveDefaults) => void;
}) {
  const [trail, setTrail] = useState<Crumb[]>([]);
  const [q, setQ] = useState("");
  const [searching, setSearching] = useState("");
  const [files, setFiles] = useState<Entry[]>([]);
  const [next, setNext] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const seq = useRef(0);
  const parent = trail.length > 0 ? trail[trail.length - 1]!.id : "";
  const [workspace, setWorkspace] = useState<Workspace>(null);
  const [mayWrite, setMayWrite] = useState(false);
  const [make, setMake] = useState<"folder" | "doc" | null>(null);
  const [name, setName] = useState("");
  const [made, setMade] = useState<Created | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let live = true;
    request("/api/drive/status", { headers: auth(token) }, { retry: true })
      .then((r) => (r.ok ? r.json() : null))
      .then((v: { workspace?: Workspace; may_write?: boolean } | null) => {
        if (!live || !v) return;
        setWorkspace(v.workspace ?? null);
        setMayWrite(v.may_write === true);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [token]);

  async function load(more: string | null) {
    const mine = ++seq.current;
    setLoading(true);
    setError("");
    const params = new URLSearchParams();
    if (parent) params.set("parent", parent);
    if (searching) params.set("q", searching);
    if (more) params.set("page", more);
    try {
      const res = await request(
        `/api/drive/files?${params}`,
        { headers: auth(token) },
        { retry: true },
      );
      const body = (await res.json().catch(() => ({}))) as {
        files?: Entry[];
        next?: string | null;
        error?: string;
      };
      if (mine !== seq.current) return;
      if (!res.ok) {
        setError(serverMessage(body.error ?? `status ${res.status}`));
        return;
      }
      setFiles((old) => (more ? [...old, ...(body.files ?? [])] : (body.files ?? [])));
      setNext(body.next ?? null);
    } catch {
      if (mine === seq.current) setError(serverMessage("Could not reach the server"));
    } finally {
      if (mine === seq.current) setLoading(false);
    }
  }

  useEffect(() => {
    void load(null);
    // The listing follows the folder and the search; `load` reads both.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [parent, searching, token]);

  async function pick(e: Entry) {
    setBusy(e.id);
    setError("");
    try {
      const res = await request("/api/drive/import", {
        method: "POST",
        headers: { ...auth(token), "content-type": "application/json" },
        body: JSON.stringify({ id: e.id, field }),
      });
      const body = (await res.json().catch(() => ({}))) as Imported & { error?: string };
      if (!res.ok) {
        setError(serverMessage(body.error ?? `status ${res.status}`));
        return;
      }
      onPicked({
        title: body.drive.name.replace(/\.pdf$/i, ""),
        location: body.drive.web_link,
        attachment: body.id,
      });
    } catch {
      setError(serverMessage("Could not reach the server"));
    } finally {
      setBusy("");
    }
  }

  const rootName = workspace?.name || t("My Drive");
  const here = searching
    ? t("Search results")
    : trail.length > 0
      ? trail[trail.length - 1]!.name
      : rootName;

  async function created(res: Response) {
    const body = (await res.json().catch(() => ({}))) as Created & { error?: string };
    if (!res.ok) {
      setError(serverMessage(body.error ?? `status ${res.status}`));
      return;
    }
    setMade(body);
    setMake(null);
    setName("");
    void load(null);
  }

  async function createNew() {
    if (!make || !name.trim()) return;
    setBusy("create");
    setError("");
    setMade(null);
    try {
      const res = await request(`/api/drive/${make === "folder" ? "folders" : "docs"}`, {
        method: "POST",
        headers: { ...auth(token), "content-type": "application/json" },
        body: JSON.stringify({ name: name.trim(), parent }),
      });
      await created(res);
    } catch {
      setError(serverMessage("Could not reach the server"));
    } finally {
      setBusy("");
    }
  }

  async function uploadFile(file: File) {
    setBusy("create");
    setError("");
    setMade(null);
    try {
      const q = new URLSearchParams({ name: file.name });
      if (parent) q.set("parent", parent);
      const res = await request(`/api/drive/upload?${q}`, {
        method: "POST",
        headers: { ...auth(token), "content-type": file.type || "application/octet-stream" },
        body: file,
      });
      await created(res);
    } catch {
      setError(serverMessage("Could not reach the server"));
    } finally {
      setBusy("");
      if (fileRef.current) fileRef.current.value = "";
    }
  }

  return (
    <Dialog
      title={t("Add a document from Google Drive")}
      size="lg"
      onClose={busy ? () => {} : onClose}
      footer={
        <Button onClick={onClose} disabled={!!busy}>
          {t("Cancel")}
        </Button>
      }
    >
      <div className="stack drive-browser">
        <form
          className="drive-search"
          onSubmit={(ev) => {
            ev.preventDefault();
            setSearching(q.trim());
          }}
        >
          <Input
            label={t("Search by name")}
            value={q}
            onChange={(ev) => setQ(ev.target.value)}
            placeholder={t("Search by name")}
          />
          <Button type="submit">{t("Search")}</Button>
          {searching && (
            <Button
              type="button"
              onClick={() => {
                setQ("");
                setSearching("");
              }}
            >
              {t("Clear")}
            </Button>
          )}
        </form>
        {!searching && (
          <nav className="drive-trail" aria-label={t("Folders")}>
            <Button size="sm" variant="ghost" onClick={() => setTrail([])}>
              {rootName}
            </Button>
            {trail.map((c, i) => (
              <span key={c.id}>
                <Icon name="chevron-right" size={14} />
                <Button size="sm" variant="ghost" onClick={() => setTrail(trail.slice(0, i + 1))}>
                  {c.name}
                </Button>
              </span>
            ))}
          </nav>
        )}
        {mayWrite && (
          <div className="drive-create">
            <Button size="sm" disabled={!!busy} onClick={() => setMake("folder")}>
              {t("New folder")}
            </Button>
            <Button size="sm" disabled={!!busy} onClick={() => setMake("doc")}>
              {t("New Google Doc")}
            </Button>
            <Button size="sm" disabled={!!busy} onClick={() => fileRef.current?.click()}>
              {t("Upload a file")}
            </Button>
            <input
              ref={fileRef}
              type="file"
              hidden
              onChange={(ev) => {
                const f = ev.target.files?.[0];
                if (f) void uploadFile(f);
              }}
            />
          </div>
        )}
        {make && (
          <form
            className="drive-search"
            onSubmit={(ev) => {
              ev.preventDefault();
              void createNew();
            }}
          >
            <Input
              label={t("Name")}
              value={name}
              autoFocus
              onChange={(ev) => setName(ev.target.value)}
            />
            <Button
              type="submit"
              variant="primary"
              loading={busy === "create"}
              disabled={!name.trim()}
            >
              {t("Create")}
            </Button>
            <Button type="button" onClick={() => setMake(null)}>
              {t("Cancel")}
            </Button>
          </form>
        )}
        {made && (
          <Notice tone="success">
            {t("Created {name}.", { name: made.name })}{" "}
            {made.web_link && (
              <a href={made.web_link} target="_blank" rel="noreferrer noopener">
                {t("Open in Google Drive")}
              </a>
            )}
          </Notice>
        )}
        {error && <Notice tone="danger">{error}</Notice>}
        <Table caption={here} dense>
          <thead>
            <tr>
              <th scope="col">{t("Name")}</th>
              <th scope="col">{t("Modified")}</th>
              <th scope="col">{t("Size")}</th>
              <th scope="col">
                <span className="sr-only">{t("Actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {files.map((f) => (
              <tr key={f.id}>
                <td>
                  {f.folder ? (
                    <Button
                      size="sm"
                      variant="ghost"
                      iconEnd="chevron-right"
                      onClick={() => {
                        setQ("");
                        setSearching("");
                        const crumb = { id: f.id, name: f.name };
                        setTrail(searching ? [crumb] : [...trail, crumb]);
                      }}
                    >
                      {f.name}
                    </Button>
                  ) : (
                    <span className={f.importable ? "" : "hint"}>{f.name}</span>
                  )}
                </td>
                <td>
                  {f.modified ? fmt.dateOf(new Date(f.modified), { dateStyle: "medium" }) : ""}
                </td>
                <td>{f.size != null ? formatBytes(f.size) : ""}</td>
                <td className="drive-actions">
                  {f.importable && (
                    <Button
                      size="sm"
                      variant="primary"
                      loading={busy === f.id}
                      disabled={!!busy}
                      onClick={() => void pick(f)}
                    >
                      {t("Add")}
                    </Button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
        {loading && <Spinner label={t("Loading")} />}
        {!loading && !error && files.length === 0 && <p className="hint">{t("Nothing here.")}</p>}
        {next && !loading && <Button onClick={() => void load(next)}>{t("Show more")}</Button>}
        <p className="hint">
          {t(
            "Google Docs, Sheets and Slides come in as PDF. Files up to the size the document field allows; nothing is changed in Drive.",
          )}
        </p>
      </div>
    </Dialog>
  );
}
