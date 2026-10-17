import { useId, useState } from "react";
import { Badge, Button, Dialog, Icon, Notice, t } from "@obop/ui";
import { call } from "../api.ts";
import { serverMessage } from "../serverMessage.ts";
import { Notes } from "./UpgradeDetails.tsx";
import type { Row, UpgradeAll } from "./types.ts";

/**
 * Update every module that has a newer version this instance can take (D63): lists what will move
 * and what stays, with the reason, then runs the server's `module.upgrade_all`. A module whose
 * update is blocked stays where it is; the others still update.
 */
export function UpdateAllDialog({
  token,
  rows,
  nameOf,
  onClose,
  onDone,
}: {
  token: string;
  /** The installed modules that have an update on offer. */
  rows: Row[];
  nameOf: (id: string) => string;
  onClose: () => void;
  onDone: (r: { updated: number; proposed: boolean }) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState("");
  const [result, setResult] = useState<UpgradeAll | null>(null);
  const bodyId = useId();
  const fits = rows.filter((r) => r.update_compatible !== false);
  const blocked = rows.filter((r) => r.update_compatible === false);

  async function go() {
    setBusy(true);
    setFailure("");
    const r = await call(token, "module.upgrade_all");
    setBusy(false);
    if (r.kind === "ok") {
      const v = r.value as UpgradeAll;
      if ((v.skipped ?? []).length === 0)
        onDone({ updated: (v.updated ?? []).length, proposed: false });
      else setResult({ updated: v.updated ?? [], skipped: v.skipped ?? [] });
    } else if (r.kind === "approval") onDone({ updated: 0, proposed: true });
    else setFailure(serverMessage(r.message));
  }

  return (
    <Dialog
      title={t("Update all compatible modules")}
      size="lg"
      describedBy={bodyId}
      className="plan-dialog"
      onClose={
        busy
          ? () => {}
          : result
            ? () => onDone({ updated: result.updated.length, proposed: false })
            : onClose
      }
      footer={
        result ? (
          <Button
            variant="primary"
            data-autofocus
            onClick={() => onDone({ updated: result.updated.length, proposed: false })}
          >
            {t("Close")}
          </Button>
        ) : (
          <>
            <Button data-autofocus onClick={onClose} disabled={busy}>
              {t("Cancel")}
            </Button>
            <Button
              variant="primary"
              loading={busy}
              disabled={fits.length === 0}
              onClick={() => void go()}
            >
              {t("{count, plural, one {Update # module} other {Update # modules}}", {
                count: fits.length,
              })}
            </Button>
          </>
        )
      }
    >
      <div id={bodyId} className="plan-body">
        {result ? (
          <>
            <Notice tone="success" title={t("Updated {count}", { count: result.updated.length })}>
              {result.updated.map((u) => `${nameOf(u.id) || u.name} ${u.version}`).join(", ")}
            </Notice>
            <Notice tone="warning" title={t("Left as they were")}>
              {t("These could not be updated and nothing was changed for them:")}
            </Notice>
            <ul className="blockers">
              {result.skipped.map((s) => (
                <li key={s.id}>
                  <strong>{nameOf(s.id)}</strong>
                  <p>{s.reason}</p>
                </li>
              ))}
            </ul>
          </>
        ) : (
          <>
            <p className="hint">
              {t(
                "Each module is updated in turn, together with the data steps it declares. A module that cannot be updated stays as it is; the others still update.",
              )}
            </p>
            {fits.length > 0 && (
              <ul className="plain upg-all-list" aria-label={t("Will be updated")}>
                {fits.map((r) => (
                  <li key={r.id}>
                    <Icon name="arrow-up" size={14} />
                    <strong>{nameOf(r.id)}</strong>
                    <span className="tabular">
                      {" "}
                      {r.version} → {r.update_available}
                    </span>
                    {(r.update_migrations ?? []).length > 0 && (
                      <Badge tone="info" dot={false} className="mod-badge">
                        {t("{count, plural, one {# data step} other {# data steps}}", {
                          count: (r.update_migrations ?? []).length,
                        })}
                      </Badge>
                    )}
                  </li>
                ))}
              </ul>
            )}
            {blocked.length > 0 && (
              <>
                <Notice tone="warning" title={t("Will stay as they are")}>
                  {t("An update here is blocked, so these modules are left alone:")}
                </Notice>
                <ul className="blockers">
                  {blocked.map((r) => (
                    <li key={r.id}>
                      <strong>{nameOf(r.id)}</strong>
                      <span className="hint tabular">
                        {" "}
                        · {r.version} → {r.update_available}
                      </span>
                      {(r.update_blockers ?? []).map((b) => (
                        <p key={b.message}>{b.message}</p>
                      ))}
                    </li>
                  ))}
                </ul>
              </>
            )}
            {failure && (
              <Notice tone="danger" title={t("The update failed")}>
                {failure}
              </Notice>
            )}
          </>
        )}
      </div>
    </Dialog>
  );
}

/** Putting a module back to the version before its last update, when the data allows it. */
export function RollbackDialog({
  token,
  row,
  title,
  onClose,
  onDone,
}: {
  token: string;
  row: Row;
  title: string;
  onClose: () => void;
  onDone: (o: { proposed: boolean }) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState("");
  const blocked = row.rollback_available === false;
  async function go() {
    setBusy(true);
    setFailure("");
    const r = await call(token, "module.rollback", { id: row.id });
    setBusy(false);
    if (r.kind === "ok") onDone({ proposed: false });
    else if (r.kind === "approval") onDone({ proposed: true });
    else setFailure(serverMessage(r.message));
  }
  return (
    <Dialog
      role="alertdialog"
      size="md"
      title={t("Roll {name} back to {version}?", {
        name: title,
        version: row.rollback_version ?? "",
      })}
      className="plan-dialog"
      onClose={busy ? () => {} : onClose}
      footer={
        <>
          <Button data-autofocus onClick={onClose} disabled={busy}>
            {blocked ? t("Close") : t("Cancel")}
          </Button>
          {!blocked && (
            <Button variant="danger" loading={busy} onClick={() => void go()}>
              {t("Roll back")}
            </Button>
          )}
        </>
      }
    >
      <div className="plan-body">
        {blocked ? (
          <Notice tone="danger" title={t("This update cannot be undone from here")}>
            {row.rollback_blocker ?? ""}
          </Notice>
        ) : (
          <>
            <p>
              {t(
                "{name} goes back to version {version}. Fields that were renamed by the update are renamed back. Records and fields the update added stay in the database; the older version ignores them.",
                { name: title, version: row.rollback_version ?? "" },
              )}
            </p>
            <Notes
              text={row.changelog ?? ""}
              label={t("Notes of the version you are leaving ({version})", {
                version: row.version,
              })}
            />
          </>
        )}
        {failure && (
          <Notice tone="danger" title={t("The rollback failed")}>
            {failure}
          </Notice>
        )}
      </div>
    </Dialog>
  );
}
