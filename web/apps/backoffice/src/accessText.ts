import { t, tx } from "@obop/ui";
import { prefixOf, roleOf, type Ladder, type Level } from "./access.ts";

// The words of access by app. A group identifier is never translated (D52); the role word is the
// part after the dot, looked up in a small generic vocabulary and otherwise spaced and capitalised.
// Every word has the context "access": "User" or "Manager" here is a rung of a ladder, not a noun
// that another screen may mean differently.

/** "device_admin" becomes "Device admin". */
export function humanise(s: string): string {
  const spaced = s.replace(/[_.-]+/g, " ").trim();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

/** The word for a role: `user` is "User", `agent` is "Agent", an unknown `cost_viewer` is "Cost viewer". */
export function roleWord(role: string): string {
  switch (role) {
    case "user":
      return tx("access", "User");
    case "manager":
      return tx("access", "Manager");
    case "agent":
      return tx("access", "Agent");
    case "driver":
      return tx("access", "Driver");
    case "reviewer":
      return tx("access", "Reviewer");
    case "admin":
      return tx("access", "Admin");
    case "finance":
      return tx("access", "Finance");
    default:
      return humanise(role);
  }
}

/** The word of the rung that a group makes: "Agent" for `helpdesk.agent`. */
export function groupWord(group: string): string {
  return roleWord(roleOf(group));
}

/** The label of a group no app owns: "Api: Device admin". */
export function otherLabel(group: string): string {
  const p = prefixOf(group);
  return p ? `${humanise(p)}: ${humanise(roleOf(group))}` : humanise(group);
}

/** The word of a level in one app. */
export function levelWord(ladder: Ladder, level: Level): string {
  if (level === "none") return t("No access");
  if (level === "custom") return tx("access", "Custom");
  return level === "manager" && ladder.manage ? groupWord(ladder.manage) : groupWord(ladder.base);
}
