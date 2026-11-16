// Projects: a project opens on how far it is, its milestones as a strip and its open tasks as a
// checklist with "Add a task" (UI plan 3.5).
import { registerRecordExtension } from "../extensions.ts";
import { PROJECT, ProjectPanel } from "../apps/projects/ProjectPanel.tsx";

registerRecordExtension(PROJECT, { panels: [ProjectPanel] });
