# Patterns added with workflow integrity, delete and server search

How the back office behaves for the three backend features that arrived with D48 and D49, and for the
two screens built on top of them. The rules in [BRAND.md](BRAND.md) and the component list in
[DESIGN-SYSTEM.md](DESIGN-SYSTEM.md) apply; this page says what is specific here. Implementation:
`web/apps/backoffice/src` (`workflow.ts`, `serverQuery.ts`, `useServer.ts`, `remove.ts`,
`DeleteDialog.tsx`, `dashboards/`, `Guide.tsx`, `markdown.ts`).

## New primitives in `@obop/ui`

| Component | Use |
|---|---|
| `ConfirmDialog` | The in-app replacement for `window.confirm` (never use the native one). States the consequence, keeps focus on Cancel, stays open to show a failure in its alert area, and can switch to a result with only a Close button. |
| `Pager` | "1–50 of 1,234", previous and next, rows per page (25, 50, 100, 200). A live region announces the range. Sits under the table and stays in view. |
| `Stat` | One number with a label and a hint: a KPI tile. Tabular figures. |
| `CodeBlock` | A code block with a copy button that announces "Copied". Highlights `toml`, `json` and `sh` from tokens, never from markup, so source text is always rendered as text. |
| `Menu` item `description` | A second line under a disabled item that says why, also its tooltip. |
| `Tooltip` `align` | `start` or `end` for controls near a panel edge. |

Each has a specimen in the Brand book (Components).

## Governed fields (a select that an action moves)

- On create, a governed field is read-only at its first option and says "A new lead starts as New; it
  moves on through actions". It is never a choice, and it is not sent.
- A target state is enabled only when an action the person may run leads there from the current state
  (`from` empty means any). `/api/meta` lists only the actions the person may run, so "not listed" and
  "not allowed" are the same thing.
- An unavailable move is not hidden and not removed from the tab order: it is `aria-disabled`, drawn
  dashed, and its tooltip gives the reason in one sentence: "No action moves a lead from Won to New".
  Native `disabled` is avoided on purpose because a disabled button has no tooltip for keyboard or
  screen-reader users.
- A move that needs a human's approval carries a `*`. For an agent the answer is a 202 and the toast
  names the plan: "Approval needed: moving lead to Won (Mark won) was proposed as plan 1a2b3c4d. Nothing
  changes until a human approves it."
- Moves call the action command, so the action's own handler runs. The server's own refusal text
  (D48) is shown verbatim, inline and as a toast.
- Kanban: a column that no action reaches from the dragged card does not accept the drop, shows
  the reason, and the "Move to" select disables it with the same reason. New cards can only be added
  to the first column.

## Delete

- A Delete button on the record (when `can_delete`) and in the list's selection bar. Both open the same
  dialog: "Delete lead “Annual support renewal”? This cannot be undone."
- A refusal because other records still point at the record stays in the dialog and lists the
  referencing models with links to the records, for models the person can open.
- Many records get a result per record. An agent can only propose a delete; the toast explains the
  plan and nothing is removed.

## Lists

- The list pages on the server through `<model>/search`. What the server cannot express runs in the
  browser on up to 5,000 loaded records (an any-field search, "contains" on a select or ref, grouping, a
  sort by a ref). `serverQuery.ts` documents the full list.
- Kanban, pivot, graph and calendar load up to 5,000 records and say so in a notice when there are more.
- The page is in the address (`page=2`); a new search starts again at page 1. Page size is a
  per-browser preference.

## Dashboards

- A tile never shows data it cannot prove the person may see: a model or field missing from
  `/api/meta` renders "No access" and nothing is requested.
- Tile filters are text: `field operator value; …`. Operators: `=`, `!=`, `~`, `>`, `>=`, `<`, `<=`,
  `in` (values separated by `|`), `is empty`, `is set`. A JSON object of exact values (the package
  documentation's form) is read as equality on each key.
