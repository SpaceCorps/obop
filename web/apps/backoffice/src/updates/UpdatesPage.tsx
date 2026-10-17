import { useCallback, useEffect, useId, useRef, useState } from "react";
import {
  Badge,
  Button,
  Dialog,
  Icon,
  IconButton,
  Notice,
  Skeleton,
  Spinner,
  fmt,
  t,
} from "@obop/ui";
import { call } from "../api.ts";
import { serverMessage } from "../serverMessage.ts";
import { prettyReq } from "../modules/status.ts";
import type { InstanceUpdate, ReleaseInfo } from "./types.ts";
import { isBusy, stateLabel } from "./status.ts";

function when(secs: number): string {
  return secs ? fmt.dateOf(new Date(secs * 1000), { dateStyle: "medium", timeStyle: "short" }) : "";
}

function CopyLine({ text }: { text: string }) {
  const [done, setDone] = useState(false);
  return (
    <span className="upd-cmd">
      <code>{text}</code>
      <IconButton
        icon="copy"
        label={done ? t("Copied") : t("Copy this line")}
        onClick={() => {
          void navigator.clipboard?.writeText(text).then(() => {
            setDone(true);
            setTimeout(() => setDone(false), 1500);
          });
        }}
      />
    </span>
  );
}

/** A step of the by-hand instructions: the leading "1. " is the list's numbering. */
function stripNumber(line: string): string {
  return line.replace(/^\d+\.\s+/, "");
}

/** Commands inside an instruction line are shown as code with their own copy button. */
function Instruction({ line }: { line: string }) {
  const text = stripNumber(line);
  const at = text.indexOf(": ");
  if (at > 0 && /^(obop|docker|sudo|pg_dump)\b/.test(text.slice(at + 2))) {
    return (
      <>
        {text.slice(0, at + 1)} <CopyLine text={text.slice(at + 2)} />
      </>
    );
  }
  return <>{text}</>;
}

function Progress({ u, onRollback }: { u: InstanceUpdate; onRollback: () => void }) {
  const busy = isBusy(u.state);
  const tone =
    u.state === "succeeded"
      ? "success"
      : u.state === "failed"
        ? "danger"
        : u.state === "rolled_back"
          ? "warning"
          : "info";
  const what =
    u.kind === "rollback"
      ? t("Going back to {to}", { to: u.target_version })
      : t("Updating from {from} to {to}", { from: u.from_version, to: u.target_version });
  return (
    <section className="upd-progress card" aria-label={t("Progress of the update")}>
      <div className="upd-progress-head">
        {busy ? <Spinner size={18} label={t("Working")} /> : null}
        <strong>{what}</strong>
        <Badge tone={tone} dot={false} className="mod-badge" data-state={u.state}>
          {stateLabel(u.state)}
        </Badge>
      </div>
      <p className="hint" role="status" aria-live="polite">
        {busy
          ? u.phase || t("Waiting for the control plane")
          : u.state === "succeeded"
            ? u.current_version === u.target_version
              ? t("This instance now runs {version}.", { version: u.current_version })
              : t("The control plane finished. This instance restarts on the new version.")
            : (u.error ?? "")}
      </p>
      {busy && (
        <p className="hint">
          {t(
            "The instance restarts while it updates and may be unreachable for a moment. This page keeps checking.",
          )}
        </p>
      )}
      {u.state === "failed" && u.error && (
        <Notice tone="danger" title={t("The update did not complete")}>
          {u.error}
        </Notice>
      )}
      {u.state === "rolled_back" && (
        <Notice tone="warning" title={t("The new version did not start; the old one is back")}>
          {u.error}
        </Notice>
      )}
      <dl className="meta-list upd-meta">
        {u.started_at > 0 && (
          <div className="meta-row">
            <dt>{t("Started")}</dt>
            <dd>
              {when(u.started_at)}
              {u.started_by ? ` · ${u.started_by}` : ""}
            </dd>
          </div>
        )}
        {u.backup && (
          <div className="meta-row">
            <dt>{t("Backup taken first")}</dt>
            <dd>
              <code>{u.backup}</code>
            </dd>
          </div>
        )}
        {u.finished_at > 0 && !busy && (
          <div className="meta-row">
            <dt>{t("Finished")}</dt>
            <dd>{when(u.finished_at)}</dd>
          </div>
        )}
      </dl>
      {u.can_rollback && (
        <div className="upd-actions">
          <Button icon="undo" onClick={onRollback}>
            {t("Roll back to {version}", { version: u.from_version })}
          </Button>
          <p className="hint">
            {t(
              "Database changes only move forward. If the older version refuses the data, restore the backup above.",
            )}
          </p>
        </div>
      )}
    </section>
  );
}

function ReleaseItem({ r, current }: { r: ReleaseInfo; current: string }) {
  const id = useId();
  const incompatible = r.modules.filter((m) => !m.compatible);
  return (
    <li className="upd-release">
      <details open={r.newer && r.installable}>
        <summary>
          <strong className="tabular">{r.version}</strong>
          {r.version === current ? (
            <Badge tone="success" dot={false} className="mod-badge">
              {t("You run this")}
            </Badge>
          ) : r.newer ? (
            r.installable ? (
              <Badge tone="info" dot={false} className="mod-badge">
                {t("Can be installed here")}
              </Badge>
            ) : (
              <Badge tone="danger" dot={false} className="mod-badge">
                {t("Blocked")}
              </Badge>
            )
          ) : (
            <Badge tone="neutral" dot={false} className="mod-badge">
              {t("Older")}
            </Badge>
          )}
          {r.published_at > 0 && <span className="hint"> {when(r.published_at)}</span>}
        </summary>
        <div className="stack" aria-labelledby={id}>
          {r.notes.trim() ? (
            <p className="upd-notes-text">{r.notes.trim()}</p>
          ) : (
            <p className="hint">{t("No release notes.")}</p>
          )}
          {r.blockers.length > 0 && (
            <Notice tone="danger" title={t("This release cannot be installed here yet")}>
              <ul className="plain">
                {r.blockers.map((b) => (
                  <li key={b}>{b}</li>
                ))}
              </ul>
            </Notice>
          )}
          {r.warnings.length > 0 && (
            <Notice tone="warning" title={t("Worth knowing")}>
              <ul className="plain">
                {r.warnings.map((w) => (
                  <li key={w}>{w}</li>
                ))}
              </ul>
            </Notice>
          )}
          {r.newer && r.modules.length > 0 && (
            <div className="table-wrap">
              <table className="table upd-modules">
                <caption className="sr-only">
                  {t("Installed modules and core {version}", { version: r.version })}
                </caption>
                <thead>
                  <tr>
                    <th scope="col">{t("Module")}</th>
                    <th scope="col">{t("Supports core")}</th>
                    <th scope="col">{t("With {version}", { version: r.version })}</th>
                  </tr>
                </thead>
                <tbody>
                  {r.modules.map((m) => (
                    <tr key={m.id}>
                      <td>
                        {m.name} <span className="hint tabular">{m.version}</span>
                      </td>
                      <td>
                        <code>{prettyReq(m.requires_core)}</code>
                      </td>
                      <td>
                        {m.compatible ? (
                          <Badge tone="success" dot={false} className="mod-badge">
                            <Icon name="check" size={12} />
                            {t("Works")}
                          </Badge>
                        ) : (
                          <Badge tone="danger" dot={false} className="mod-badge">
                            <Icon name="warning" size={12} />
                            {t("Would not run")}
                          </Badge>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {r.newer && r.modules.length === 0 && incompatible.length === 0 && (
            <p className="hint">{t("No packages are installed, so there is nothing to check.")}</p>
          )}
        </div>
      </details>
    </li>
  );
}

/**
 * Customize > Updates: the core version this instance runs against what has been published, the
 * notes, whether the installed modules fit a newer core (from their declared core ranges), a
 * backup, and one button that asks the control plane to run the upgrade (D63). A self-hosted
 * instance gets the commands instead. Progress is followed live; the result stays on the page.
 */
export function UpdatesPage({
  token,
  notify,
}: {
  token: string;
  notify: (text: string, tone?: "ok" | "error" | "info") => void;
}) {
  const [u, setU] = useState<InstanceUpdate | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [dialog, setDialog] = useState<"start" | "rollback" | null>(null);
  const live = useRef(true);

  const check = useCallback(async () => {
    setLoading(true);
    const r = await call(token, "instance_update.check");
    if (!live.current) return;
    setLoading(false);
    if (r.kind === "ok") {
      setU(r.value as InstanceUpdate);
      setError("");
    } else if (r.kind === "error") setError(serverMessage(r.message));
  }, [token]);

  useEffect(() => {
    live.current = true;
    void check();
    return () => {
      live.current = false;
    };
  }, [check]);

  // Follow a running update. The instance restarts during its own update: a failed poll is just
  // "not back yet".
  const busy = u ? isBusy(u.state) : false;
  useEffect(() => {
    if (!busy) return;
    const timer = setInterval(async () => {
      const r = await call(token, "instance_update.status");
      if (!live.current) return;
      if (r.kind === "ok") {
        const s = r.value as Partial<InstanceUpdate>;
        setU((old) => (old ? { ...old, ...s } : old));
        if (s.state && !isBusy(s.state)) void check();
      }
    }, 2000);
    return () => clearInterval(timer);
  }, [busy, token, check]);

  if (!u)
    return (
      <div className="stack gap-lg upd">
        <header className="mods-head">
          <div className="mods-head-text">
            <h2>{t("Updates")}</h2>
          </div>
        </header>
        {error ? (
          <Notice
            tone="danger"
            title={t("Updates could not be loaded")}
            action={
              <Button size="sm" onClick={() => void check()}>
                {t("Try again")}
              </Button>
            }
          >
            {error}
          </Notice>
        ) : (
          <div aria-busy="true" aria-label={t("Checking for updates")}>
            <Skeleton rows={4} cols={2} />
          </div>
        )}
      </div>
    );

  const newest = u.releases.find((r) => r.newer);
  const target = u.releases.find((r) => r.newer && r.installable);
  const managed = u.mode === "managed";
  const showProgress = u.state !== "idle";

  return (
    <div className="stack gap-lg upd">
      <header className="mods-head">
        <div className="mods-head-text">
          <h2>{t("Updates")}</h2>
          <p className="hint">
            {t(
              "Keep the instance itself current. Before anything moves, every installed module is checked against the new core, and a backup is taken. Nothing is contacted until you check.",
            )}
          </p>
        </div>
        <Badge
          tone="accent"
          dot={false}
          className="mods-core"
          title={t("The running core version")}
        >
          <Icon name="shield-check" size={14} />
          {t("Core {version}", { version: u.current_version })}
        </Badge>
      </header>

      {u.feed_error && (
        <Notice
          tone="warning"
          title={t("Could not check for updates")}
          action={
            <Button size="sm" onClick={() => void check()} loading={loading}>
              {t("Check again")}
            </Button>
          }
        >
          {u.feed_error}
        </Notice>
      )}
      {error && (
        <Notice tone="danger" title={t("Something went wrong")}>
          {error}
        </Notice>
      )}

      <section className="card upd-now" aria-label={t("Current version")}>
        <div className="upd-now-main">
          <div>
            <h3 className="upd-version">
              {u.update_available && newest
                ? t("Version {version} is available", { version: newest.version })
                : u.feed_error
                  ? t("Version {version}", { version: u.current_version })
                  : t("You are up to date")}
            </h3>
            <p className="hint tabular">
              {t("You run {version}.", { version: u.current_version })}{" "}
              {u.checked_at > 0 && !u.feed_error
                ? t("Checked {when}.", { when: when(u.checked_at) })
                : ""}
            </p>
          </div>
          <div className="upd-buttons">
            <Button icon="replay" onClick={() => void check()} loading={loading}>
              {t("Check for updates")}
            </Button>
            {managed && target && !busy && (
              <Button
                variant="primary"
                icon="arrow-up"
                onClick={() => setDialog("start")}
                disabled={isBusy(u.state)}
              >
                {t("Update now")}
              </Button>
            )}
          </div>
        </div>
        <div className="mod-badges">
          <Badge tone={managed ? "info" : "neutral"} dot={false} className="mod-badge">
            {managed ? t("Managed by a control plane") : t("Self-hosted")}
          </Badge>
          {u.backup_dir ? (
            <Badge tone="success" dot={false} className="mod-badge">
              <Icon name="download" size={12} />
              {t("Backups go to {dir}", { dir: u.backup_dir })}
            </Badge>
          ) : managed ? (
            <Badge tone="warning" dot={false} className="mod-badge">
              <Icon name="warning" size={12} />
              {t("No backup directory is set")}
            </Badge>
          ) : null}
        </div>
        {u.update_available && newest && !newest.installable && (
          <Notice tone="danger" title={t("The newest release cannot be installed here yet")}>
            <ul className="plain">
              {newest.blockers.map((b) => (
                <li key={b}>{b}</li>
              ))}
            </ul>
            {target && (
              <p>{t("Version {version} can be installed now.", { version: target.version })}</p>
            )}
          </Notice>
        )}
        {managed && !u.backup_dir && target && (
          <Notice tone="warning" title={t("No automatic backup")}>
            {t(
              "OBOP_BACKUP_DIR is not set, so a backup cannot be taken here. You can still update if you have your own backup (obop backup, or pg_dump).",
            )}
          </Notice>
        )}
        {!managed && (
          <p className="hint">
            {t(
              "This instance is not managed by a control plane, so it cannot update itself. Follow the steps below.",
            )}
          </p>
        )}
      </section>

      {showProgress && <Progress u={u} onRollback={() => setDialog("rollback")} />}

      {u.releases.length > 0 && (
        <section className="stack" aria-labelledby="upd-rel-h">
          <h3 id="upd-rel-h" className="overline">
            {t("Releases")}
          </h3>
          <ul className="plain upd-releases">
            {u.releases.slice(0, 8).map((r) => (
              <ReleaseItem key={r.version} r={r} current={u.current_version} />
            ))}
          </ul>
        </section>
      )}

      {u.dropped.length > 0 && (
        <Notice tone="warning" title={t("Some releases in the feed were left out")}>
          {u.dropped.join(" ")}
        </Notice>
      )}

      <section className="stack" aria-labelledby="upd-hand-h">
        <h3 id="upd-hand-h" className="overline">
          {managed ? t("Prefer to update by hand?") : t("Update by hand")}
        </h3>
        <ol className="upd-steps">
          {u.instructions.slice(0, 5).map((l) => (
            <li key={l}>
              <Instruction line={l} />
            </li>
          ))}
        </ol>
        <p className="hint">{u.instructions[5] ?? ""}</p>
        <p className="hint">
          {t("The same check is available as {command}.", { command: "obop update check" })}
        </p>
      </section>

      <p className="hint">
        {t(
          "An agent can only propose an update: it never runs until a person approves it. Every request and its result is written to the security log.",
        )}
      </p>

      {dialog === "start" && target && (
        <StartDialog
          token={token}
          u={u}
          release={target}
          onClose={() => setDialog(null)}
          onDone={(r) => {
            setDialog(null);
            if (r.proposed) notify(t("Waiting for a human to approve"), "info");
            else {
              notify(t("Update requested"), "ok");
              setU((old) =>
                old ? { ...old, ...r.status, state: r.status?.state ?? "running" } : old,
              );
            }
          }}
        />
      )}
      {dialog === "rollback" && (
        <RollbackDialog
          token={token}
          u={u}
          onClose={() => setDialog(null)}
          onDone={(r) => {
            setDialog(null);
            if (r.proposed) notify(t("Waiting for a human to approve"), "info");
            else {
              notify(t("Rollback requested"), "ok");
              void check();
            }
          }}
        />
      )}
    </div>
  );
}

function StartDialog({
  token,
  u,
  release,
  onClose,
  onDone,
}: {
  token: string;
  u: InstanceUpdate;
  release: ReleaseInfo;
  onClose: () => void;
  onDone: (r: { proposed: boolean; status?: Partial<InstanceUpdate> }) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState("");
  const [own, setOwn] = useState(false);
  const bodyId = useId();
  const needsConfirm = !u.backup_dir;
  async function go() {
    setBusy(true);
    setFailure("");
    const r = await call(token, "instance_update.start", {
      version: release.version,
      skip_backup: needsConfirm && own,
    });
    setBusy(false);
    if (r.kind === "ok") onDone({ proposed: false, status: r.value as Partial<InstanceUpdate> });
    else if (r.kind === "approval") onDone({ proposed: true });
    else setFailure(serverMessage(r.message));
  }
  return (
    <Dialog
      title={t("Update to {version}?", { version: release.version })}
      size="md"
      describedBy={bodyId}
      className="plan-dialog"
      onClose={busy ? () => {} : onClose}
      footer={
        <>
          <Button data-autofocus onClick={onClose} disabled={busy}>
            {t("Cancel")}
          </Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={needsConfirm && !own}
            onClick={() => void go()}
          >
            {t("Update now")}
          </Button>
        </>
      }
    >
      <div id={bodyId} className="plan-body">
        <p>
          {t(
            "{from} becomes {to}. The control plane redeploys this instance and waits for the new version to answer; if it does not, the old version is put back.",
            { from: u.current_version, to: release.version },
          )}
        </p>
        <ul className="plain upd-checks">
          <li>
            <Icon name="check-circle" size={16} />{" "}
            {t(
              "{count, plural, one {# installed module is} other {# installed modules are}} compatible.",
              {
                count: release.modules.filter((m) => m.compatible).length,
              },
            )}
          </li>
          <li>
            <Icon name={u.backup_dir ? "check-circle" : "warning"} size={16} />{" "}
            {u.backup_dir
              ? t("A backup is taken first, into {dir}.", { dir: u.backup_dir })
              : t("No backup can be taken here.")}
          </li>
          <li>
            <Icon name="info" size={16} />{" "}
            {t(
              "The database is migrated forward when the new version starts. The way back for the data is the backup.",
            )}
          </li>
        </ul>
        {release.warnings.length > 0 && (
          <Notice tone="warning" title={t("Worth knowing")}>
            {release.warnings.join(" ")}
          </Notice>
        )}
        {needsConfirm && (
          <label className="check">
            <input type="checkbox" checked={own} onChange={(e) => setOwn(e.target.checked)} />
            <span>{t("I have my own backup of this database")}</span>
          </label>
        )}
        {failure && (
          <Notice tone="danger" title={t("The update was not started")}>
            {failure}
          </Notice>
        )}
      </div>
    </Dialog>
  );
}

function RollbackDialog({
  token,
  u,
  onClose,
  onDone,
}: {
  token: string;
  u: InstanceUpdate;
  onClose: () => void;
  onDone: (r: { proposed: boolean }) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState("");
  async function go() {
    setBusy(true);
    setFailure("");
    const r = await call(token, "instance_update.rollback");
    setBusy(false);
    if (r.kind === "ok") onDone({ proposed: false });
    else if (r.kind === "approval") onDone({ proposed: true });
    else setFailure(serverMessage(r.message));
  }
  return (
    <Dialog
      role="alertdialog"
      title={t("Go back to {version}?", { version: u.from_version })}
      size="md"
      className="plan-dialog"
      onClose={busy ? () => {} : onClose}
      footer={
        <>
          <Button data-autofocus onClick={onClose} disabled={busy}>
            {t("Cancel")}
          </Button>
          <Button variant="danger" loading={busy} onClick={() => void go()}>
            {t("Roll back")}
          </Button>
        </>
      }
    >
      <div className="plan-body">
        <p>
          {t(
            "The control plane puts the previous image back. Changes the update made to the database are not undone. If {version} refuses the data, restore the backup taken before the update.",
            { version: u.from_version },
          )}
        </p>
        {u.backup && (
          <p className="hint">
            {t("Backup:")} <code>{u.backup}</code>
          </p>
        )}
        {failure && (
          <Notice tone="danger" title={t("The rollback was not started")}>
            {failure}
          </Notice>
        )}
      </div>
    </Dialog>
  );
}
