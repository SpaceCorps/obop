import { Icon, t } from "@obop/ui";
import { StatusChip } from "./badges.tsx";
import { prettyReq, type TreeNode } from "./status.ts";

/**
 * The dependencies of a module as nested lists with connecting lines. It is a plain `ul` tree, so a
 * screen reader reads the nesting; the line is decoration. Each module shows the range that its
 * parent requires, the version installed here and a status chip.
 */
export function DependencyTree({
  nodes,
  hrefFor,
  label,
}: {
  nodes: TreeNode[];
  /** Where a module's own page is, or null for one this instance does not know. */
  hrefFor?: (id: string) => string | null;
  label: string;
}) {
  if (!nodes.length)
    return <p className="hint dep-none">{t("This module needs no other module.")}</p>;
  return (
    <ul className="dep-tree" aria-label={label}>
      {nodes.map((n, i) => (
        <Branch key={`${n.id}-${i}`} node={n} hrefFor={hrefFor} />
      ))}
    </ul>
  );
}

function Branch({ node, hrefFor }: { node: TreeNode; hrefFor?: (id: string) => string | null }) {
  const href = hrefFor?.(node.id) ?? null;
  const versionText = node.installedVersion
    ? t("installed {version}", { version: node.installedVersion })
    : node.availableVersion
      ? t("not installed, {version} available", { version: node.availableVersion })
      : t("not installed");
  return (
    <li className="dep-node">
      <div className="dep-line">
        <span className="dep-name">
          {href ? <a href={href}>{node.name}</a> : <strong>{node.name}</strong>}
        </span>
        <span className="dep-range">
          {t("needs")} <code>{prettyReq(node.requirement)}</code>
        </span>
        <span className="dep-have hint">{versionText}</span>
        <StatusChip status={node.status} />
        {node.cycle && (
          <span className="hint">
            <Icon name="replay" size={12} /> {t("already listed above")}
          </span>
        )}
      </div>
      {node.children.length > 0 && (
        <ul className="dep-tree" aria-label={t("Dependencies of {name}", { name: node.name })}>
          {node.children.map((c, i) => (
            <Branch key={`${c.id}-${i}`} node={c} hrefFor={hrefFor} />
          ))}
        </ul>
      )}
    </li>
  );
}

/** The root of the tree: what the module needs from the core. */
export function CoreRequirement({
  name,
  requirement,
  core,
  compatible,
}: {
  name: string;
  requirement: string;
  core: string;
  compatible: boolean;
}) {
  return (
    <div className="dep-core" role="group" aria-label={t("Core requirement")}>
      <Icon name="shield-check" size={16} />
      <span>
        {t("{name} needs core {requirement}", { name, requirement: prettyReq(requirement) })}
      </span>
      <span className="hint">{t("you run {version}", { version: core })}</span>
      <StatusChip status={compatible ? "satisfied" : "conflict"} />
    </div>
  );
}
