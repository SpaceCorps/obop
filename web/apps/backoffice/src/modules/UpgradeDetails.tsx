import { Badge, Icon, t } from "@obop/ui";
import { prettyReq } from "./status.ts";
import type { Change, StepDependent } from "./types.ts";

/** Release notes: plain text, shown as written. */
export function Notes({ text, label }: { text: string; label: string }) {
  if (!text.trim()) return null;
  return (
    <section className="upg-notes" aria-label={label}>
      <h4 className="overline">{label}</h4>
      <p className="upg-notes-text">{text.trim()}</p>
    </section>
  );
}

function changeIcon(c: Change): "plus" | "minus" | "edit" {
  if (c.kind.endsWith("_added")) return "plus";
  if (c.kind.endsWith("_removed")) return "minus";
  return "edit";
}

/**
 * What an update does to the models (D63): additive changes need nothing, breaking ones are
 * marked and say which declared migration step resolves them.
 */
export function ChangeList({ changes, migrations }: { changes: Change[]; migrations: string[] }) {
  if (changes.length === 0 && migrations.length === 0)
    return (
      <p className="hint">{t("The records keep their shape: nothing changes in the data.")}</p>
    );
  return (
    <div className="stack upg-changes">
      <ul className="plain upg-change-list" aria-label={t("Changes to the data")}>
        {changes.map((c, i) => (
          <li key={`${c.kind}-${c.model}-${c.field ?? ""}-${i}`}>
            <Icon name={changeIcon(c)} size={14} />
            <span>
              <code>{c.model}</code> · {c.detail}
              {c.breaking && (
                <>
                  {" "}
                  <Badge tone={c.resolved_by ? "info" : "danger"} dot={false} className="mod-badge">
                    {c.resolved_by ? t("Handled by a data step") : t("Breaking")}
                  </Badge>
                </>
              )}
              {c.resolved_by && <span className="hint"> → {c.resolved_by}</span>}
            </span>
          </li>
        ))}
      </ul>
      {migrations.length > 0 && (
        <div className="upg-migrations">
          <h4 className="overline">{t("Data steps that run with the update")}</h4>
          <ol className="upg-migration-list">
            {migrations.map((m) => (
              <li key={m}>{m}</li>
            ))}
          </ol>
          <p className="hint">
            {t(
              "The steps and the new version are written in one operation. If anything fails, nothing changes.",
            )}
          </p>
        </div>
      )}
    </div>
  );
}

/**
 * The installed modules that depend on the one being changed and the versions of it each one
 * supports (D63). A dependent whose range the new version leaves blocks the update.
 */
export function DependentsTable({
  dependents,
  version,
  nameOf,
}: {
  dependents: StepDependent[];
  version: string;
  nameOf: (id: string) => string;
}) {
  if (dependents.length === 0)
    return <p className="hint">{t("No installed module depends on this one.")}</p>;
  return (
    <div className="table-wrap">
      <table className="table upg-dependents">
        <caption className="sr-only">
          {t("Installed modules that depend on this one, and the versions they support")}
        </caption>
        <thead>
          <tr>
            <th scope="col">{t("Module")}</th>
            <th scope="col">{t("Supports")}</th>
            <th scope="col">{t("With {version}", { version })}</th>
          </tr>
        </thead>
        <tbody>
          {dependents.map((d) => (
            <tr key={d.id}>
              <td>
                {nameOf(d.id) || d.name} <span className="hint tabular">{d.version}</span>
              </td>
              <td>
                <code>{prettyReq(d.requirement)}</code>
              </td>
              <td>
                {d.satisfied ? (
                  <Badge tone="success" dot={false} className="mod-badge">
                    <Icon name="check" size={12} />
                    {t("Still supported")}
                  </Badge>
                ) : d.replaced_in_plan ? (
                  <Badge tone="info" dot={false} className="mod-badge">
                    {t("Updated in this plan")}
                  </Badge>
                ) : (
                  <Badge tone="danger" dot={false} className="mod-badge">
                    <Icon name="warning" size={12} />
                    {t("Would break")}
                  </Badge>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
