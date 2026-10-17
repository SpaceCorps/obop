import { Badge, Icon, t } from "@obop/ui";
import {
  STATUS_TONE,
  coreBadge,
  dependencyStatus,
  licenseLabel,
  maturityLabel,
  maturityTone,
  sourceLabel,
  statusLabel,
  type DepStatus,
} from "./status.ts";
import type { Dep, Row } from "./types.ts";

/** The licence of a module. A proprietary or custom licence is flagged so nobody is surprised. */
export function LicenseBadge({ row }: { row: Pick<Row, "license" | "license_kind"> }) {
  const flagged = row.license_kind === "proprietary" || row.license_kind === "custom";
  const none = row.license_kind === "none" || !row.license;
  return (
    <Badge
      tone={flagged || none ? "warning" : "neutral"}
      dot={false}
      className="mod-badge"
      title={
        row.license_kind === "proprietary"
          ? t("Proprietary: the terms are not open source")
          : row.license_kind === "custom"
            ? t("Custom licence terms: read them before installing")
            : undefined
      }
    >
      {flagged && <Icon name="warning" size={12} />}
      {licenseLabel(row)}
    </Badge>
  );
}

export function MaturityBadge({ maturity }: { maturity: string }) {
  if (!maturity) return null;
  return (
    <Badge tone={maturityTone(maturity)} className="mod-badge">
      {maturityLabel(maturity)}
    </Badge>
  );
}

/** Green when the running core fits, red with both versions when it does not. */
export function CoreBadge({
  row,
  core,
}: {
  row: Pick<Row, "compatible" | "requires_core">;
  core: string;
}) {
  const b = coreBadge(row, core);
  return (
    <Badge tone={b.tone} className="mod-badge">
      {b.text}
    </Badge>
  );
}

export function SourceBadge({ source }: { source: string }) {
  return (
    <Badge tone="neutral" dot={false} className="mod-badge">
      {sourceLabel(source)}
    </Badge>
  );
}

/** The chip of a dependency: satisfied, built in, will be installed, missing or in conflict. */
export function StatusChip({ status }: { status: DepStatus }) {
  const tone = STATUS_TONE[status];
  const icon =
    tone === "danger"
      ? "x-circle"
      : tone === "success"
        ? "check-circle"
        : tone === "info"
          ? "download"
          : "lock";
  return (
    <Badge
      tone={tone === "neutral" ? "neutral" : tone}
      dot={false}
      className="mod-badge status-chip"
    >
      <Icon name={icon} size={12} />
      {statusLabel(status)}
    </Badge>
  );
}

export function DepChip({ dep }: { dep: Dep }) {
  return <StatusChip status={dependencyStatus(dep)} />;
}
