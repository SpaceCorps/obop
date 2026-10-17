import { t } from "@obop/ui";
import type { UpdateState } from "./types.ts";

/** A request that has been made and has not finished. */
export function isBusy(state: string): boolean {
  return state === "requested" || state === "running";
}

export function stateLabel(state: UpdateState | string): string {
  switch (state) {
    case "requested":
      return t("Requested");
    case "running":
      return t("Running");
    case "succeeded":
      return t("Done");
    case "failed":
      return t("Failed");
    case "rolled_back":
      return t("Rolled back");
    default:
      return t("Idle");
  }
}
