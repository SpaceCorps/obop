import { useEffect, useId, useRef, useState } from "react";
import { Badge, Button, Dialog, Icon, Notice, Skeleton, t } from "@obop/ui";
import { call } from "../api.ts";
import { serverMessage } from "../serverMessage.ts";
import { LicenseBadge } from "./badges.tsx";
import { ChangeList, DependentsTable, Notes } from "./UpgradeDetails.tsx";
import { preview } from "./data.ts";
import { prettyReq } from "./status.ts";
import type { Blocker, Plan, PlanStep, Row } from "./types.ts";

/** What a plan is asked for: a marketplace package, or the text of an uploaded obop.toml. */
export type PlanRequest = { id: string } | { manifest: string };

export type PlanOutcome = { installed: string[]; proposed: boolean; rootName: string };

function blockerTitle(kind: string): string {
  switch (kind) {
    case "core":
      return t("Core version");
    case "missing":
      return t("Missing module");
    case "version":
      return t("Version");
    case "conflict":
      return t("Conflict");
    case "cycle":
      return t("Dependency cycle");
    case "downgrade":
      return t("Downgrade");
    case "installed":
      return t("Already installed");
    case "builtin":
      return t("Built in");
    case "refs":
      return t("Missing link to another module");
    case "unavailable":
      return t("Marketplace unavailable");
    case "dependents":
      return t("Needed by other modules");
    case "data":
      return t("Existing records");
    default:
      return t("Problem");
  }
}

export function BlockerList({
  blockers,
  nameOf,
}: {
  blockers: Blocker[];
  nameOf: (id: string) => string;
}) {
  return (
    <ul className="blockers">
      {blockers.map((b, i) => (
        <li key={`${b.module}-${b.kind}-${i}`}>
          <strong>{blockerTitle(b.kind)}</strong>
          <span className="hint"> · {nameOf(b.module)}</span>
          <p>{b.message}</p>
        </li>
      ))}
    </ul>
  );
}

function actionBadge(a: PlanStep["action"]) {
  return a === "install" ? (
    <Badge tone="info" dot={false} className="mod-badge">
      <Icon name="download" size={12} />
      {t("Install")}
    </Badge>
  ) : a === "upgrade" ? (
    <Badge tone="accent" dot={false} className="mod-badge">
      <Icon name="arrow-up" size={12} />
      {t("Upgrade")}
    </Badge>
  ) : (
    <Badge tone="neutral" dot={false} className="mod-badge">
      <Icon name="check" size={12} />
      {t("Already installed")}
    </Badge>
  );
}

/** The steps of a plan, in the order they run. Used by the install dialog and by the upload check. */
export function PlanSteps({ plan, nameOf }: { plan: Plan; nameOf: (id: string) => string }) {
  return (
    <ol className="plan-steps" aria-label={t("Steps, in the order they run")}>
      {plan.steps.map((s) => (
        <li key={s.id} className={s.action === "skip" ? "skip" : undefined}>
          <div className="plan-head">
            {actionBadge(s.action)}
            <strong>{nameOf(s.id) || s.name}</strong>
            <span className="tabular plan-version">
              {s.from_version && s.action === "upgrade"
                ? t("{from} to {to}", { from: s.from_version, to: s.version })
                : s.version}
            </span>
            <LicenseBadge row={{ license: s.license, license_kind: s.license_kind }} />
          </div>
          <p className="hint plan-why">
            {s.requested ? t("The module you asked for.") : s.reason}
            {s.requires_core && s.requires_core !== "*" && (
              <> {t("Needs core {requirement}.", { requirement: prettyReq(s.requires_core) })}</>
            )}
          </p>
          {s.dependencies.length > 0 && (
            <ul className="plan-deps" aria-label={t("Dependencies of {name}", { name: s.name })}>
              {s.dependencies.map((d) => (
                <li key={d.id}>
                  {d.name} <code>{prettyReq(d.requirement)}</code>
                </li>
              ))}
            </ul>
          )}
          {s.action === "upgrade" && (
            <div className="stack upg-step">
              <Notes
                text={s.changelog ?? ""}
                label={t("What is new in {version}", { version: s.version })}
              />
              <section className="stack" aria-label={t("What changes in the data")}>
                <h4 className="overline">{t("What changes in the data")}</h4>
                <ChangeList changes={s.changes ?? []} migrations={s.migrations ?? []} />
              </section>
              <section className="stack" aria-label={t("Modules that depend on this one")}>
                <h4 className="overline">{t("Modules that depend on this one")}</h4>
                <DependentsTable
                  dependents={s.dependents ?? []}
                  version={s.version}
                  nameOf={nameOf}
                />
              </section>
            </div>
          )}
        </li>
      ))}
    </ol>
  );
}

export function installLabel(plan: Plan, upgrade: boolean): string {
  const work = plan.steps.filter((s) => s.action !== "skip").length;
  if (work <= 1) return upgrade ? t("Upgrade") : t("Install");
  return t("{count, plural, one {Install # module} other {Install # modules}}", { count: work });
}

/**
 * The plan dialog (D55): says exactly what will be installed or upgraded, in order, with versions
 * and licences, and names anything that stops it. Nothing happens until the person confirms; one
 * confirmation covers the whole plan, which the server applies as one operation.
 */
export function PlanDialog({
  token,
  request,
  title,
  nameOf,
  rows,
  onClose,
  onDone,
}: {
  token: string;
  request: PlanRequest;
  title: string;
  nameOf: (id: string) => string;
  rows: Row[];
  onClose: () => void;
  onDone: (o: PlanOutcome) => void;
}) {
  const [state, setState] = useState<{ plan: Plan } | { error: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState("");
  const bodyId = useId();
  const key = JSON.stringify(request);
  const asked = useRef(request);
  asked.current = request;
  useEffect(() => {
    let live = true;
    setState(null);
    void preview(token, asked.current).then((r) => live && setState(r));
    return () => {
      live = false;
    };
  }, [token, key]);

  const plan = state && "plan" in state ? state.plan : null;
  const root = plan?.steps.find((s) => s.requested);
  const upgrade = root?.action === "upgrade";
  const work = plan?.steps.filter((s) => s.action !== "skip") ?? [];
  const nm = (id: string) => nameOf(id) || rows.find((r) => r.id === id)?.name || id;

  async function confirm() {
    if (!plan) return;
    setBusy(true);
    setFailure("");
    // An update goes through module.upgrade: the same plan, audited as an update (D63).
    const r =
      "id" in request
        ? upgrade
          ? await call(token, "module.upgrade", { id: request.id })
          : await call(token, "marketplace_listing.install", { id: request.id })
        : await call(token, "module.install", { manifest: request.manifest });
    setBusy(false);
    const rootName = root ? nameOf(root.id) || root.name : "";
    if (r.kind === "ok") onDone({ installed: work.map((s) => s.id), proposed: false, rootName });
    else if (r.kind === "approval") onDone({ installed: [], proposed: true, rootName });
    else setFailure(serverMessage(r.message));
  }

  return (
    <Dialog
      title={title}
      size="lg"
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
            disabled={!plan || !plan.ok || work.length === 0}
            onClick={() => void confirm()}
          >
            {plan ? installLabel(plan, upgrade) : t("Install")}
          </Button>
        </>
      }
    >
      <div id={bodyId} className="plan-body">
        {!state && (
          <div aria-busy="true" aria-label={t("Working out what this needs")}>
            <Skeleton rows={3} cols={2} />
          </div>
        )}
        {state && "error" in state && (
          <Notice tone="danger" title={t("The plan could not be made")}>
            {serverMessage(state.error)}
          </Notice>
        )}
        {plan && (
          <>
            <p className="hint">
              {t("Core {version} is running.", { version: plan.core_version })}{" "}
              {plan.ok
                ? work.length === 0
                  ? t("Nothing needs to change.")
                  : t("Nothing is changed until you confirm. These steps run in this order:")
                : t("This cannot be installed here:")}
            </p>
            {plan.blockers.length > 0 && (
              <Notice tone="danger" title={t("Blocked")}>
                {t("Fix these first; the install is refused while they stand.")}
              </Notice>
            )}
            {plan.blockers.length > 0 && <BlockerList blockers={plan.blockers} nameOf={nm} />}
            {plan.steps.length > 0 && <PlanSteps plan={plan} nameOf={nm} />}
            {plan.warnings.length > 0 && (
              <Notice tone="warning" title={t("Worth knowing")}>
                {plan.warnings.join(" ")}
              </Notice>
            )}
            {plan.ok && root && work.length > 1 && (
              <p className="hint plan-foot">
                {t(
                  "Dependencies are installed together with the module, in one operation; if a step fails nothing is left half installed.",
                )}
              </p>
            )}
          </>
        )}
        {failure && (
          <Notice tone="danger" title={t("The install failed")}>
            {failure}
          </Notice>
        )}
      </div>
    </Dialog>
  );
}

/** Removing a module: what depends on it blocks the removal, and says so. */
export function UninstallDialog({
  token,
  row,
  title,
  nameOf,
  onClose,
  onDone,
}: {
  token: string;
  row: Row;
  title: string;
  nameOf: (id: string) => string;
  onClose: () => void;
  onDone: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState("");
  const blocked = (row.uninstall_blockers ?? []).length > 0 || row.uninstallable === false;
  async function go() {
    setBusy(true);
    const r = await call(token, "module.uninstall", { id: row.id });
    setBusy(false);
    if (r.kind === "ok") onDone();
    else if (r.kind === "approval") onDone();
    else setFailure(serverMessage(r.message));
  }
  return (
    <Dialog
      role="alertdialog"
      size="md"
      title={t("Uninstall {name}?", { name: title })}
      className="plan-dialog"
      onClose={busy ? () => {} : onClose}
      footer={
        <>
          <Button data-autofocus onClick={onClose} disabled={busy}>
            {blocked ? t("Close") : t("Cancel")}
          </Button>
          {!blocked && (
            <Button variant="danger" loading={busy} onClick={() => void go()}>
              {t("Uninstall")}
            </Button>
          )}
        </>
      }
    >
      <div className="plan-body">
        {blocked ? (
          <>
            <Notice tone="danger" title={t("Other modules need {name}", { name: title })}>
              {t("Uninstall these first, then {name} can be removed.", { name: title })}
            </Notice>
            <ul className="blockers">
              {row.dependents.map((d) => (
                <li key={d}>
                  <strong>{nameOf(d)}</strong>
                  <span className="hint"> · {t("needs {name}", { name: title })}</span>
                </li>
              ))}
            </ul>
            {(row.uninstall_blockers ?? []).map((b) => (
              <p key={b.message} className="hint">
                {b.message}
              </p>
            ))}
          </>
        ) : (
          <p>
            {t(
              "The app leaves the launcher and its commands stop working. Its records stay in the database, so installing it again brings them back.",
            )}
          </p>
        )}
        {failure && (
          <Notice tone="danger" title={t("The removal failed")}>
            {failure}
          </Notice>
        )}
      </div>
    </Dialog>
  );
}
