import type { ReactNode } from "react";
import { t } from "@obop/ui";
import { isWithheldError } from "../serverQuery.ts";
import { serverMessage } from "../serverMessage.ts";
import type { Aggregated } from "../useAggregate.ts";
import { EmptyState, Skeleton } from "../ui.tsx";

/** What a view that reads an aggregate shows until the numbers are there, or when they cannot be. */
export function AggState({ agg, children }: { agg: Aggregated; children: ReactNode }) {
  if (agg.error)
    return (
      <EmptyState
        title={
          isWithheldError(agg.error)
            ? t("This view uses a field you cannot read")
            : t("Could not load the numbers")
        }
        hint={serverMessage(agg.error)}
        action={{ label: t("Try again"), onClick: agg.retry }}
      />
    );
  if (!agg.loaded) return <Skeleton rows={6} cols={4} />;
  return <>{children}</>;
}
