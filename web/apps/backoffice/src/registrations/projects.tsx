// Projects: a project opens on how far it is, its milestones as a strip and its open tasks as a
// checklist with "Add a task" (UI plan 3.5).
import { registerRecordExtension } from "../extensions.ts";
import { PROJECT, ProjectPanel } from "../apps/projects/ProjectPanel.tsx";
import { BillTimeAction } from "../apps/projects/BillTime.tsx";

// "Bill time" turns the project's approved, billable time into a draft invoice (K62).
registerRecordExtension(PROJECT, { panels: [ProjectPanel], actions: [BillTimeAction] });
