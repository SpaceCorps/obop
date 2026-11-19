import { t } from "@obop/ui";
import { Icon } from "./ui.tsx";

/**
 * The shield that marks an action an agent may only propose (a person approves it). The one
 * marker of that idea: record actions and the next step on a board card both draw it (Q14).
 */
export function ApprovalMark() {
  const text = t("An agent needs a person's approval for this");
  return (
    <span className="approval-mark" role="img" aria-label={text} title={text}>
      <Icon name="shield" size={13} />
    </span>
  );
}
