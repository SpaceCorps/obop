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
- Kanban, pivot, graph and calendar ask the server for counts, sums and pages (`aggregate`, D4) instead of
  loading records: a kanban column shows its count and sum from one aggregate and its first 40 cards,
  "Load more" asks for 40 more; the calendar asks for the days on screen; pivot and graph use the
  grouped numbers. A search the server cannot express (any-field search, "contains" on a select) falls
  back to loading up to 5,000 records, with the notice that says so.
- "Select all N matching" appears when the whole page is selected and more records match. Set-field and
  delete then run over every matching record (at most 5,000 per operation, with a message when there
  are more) in batches of 8 with a progress bar, Cancel between batches, and a result per record;
  agents still only propose, and the dialog says how many records are involved. CSV export streams in
  pages of 1,000 (at most 20,000 rows).
- The page is in the address (`page=2`); a new search starts again at page 1. Page size is a
  per-browser preference.

## Dashboards

- A tile never shows data it cannot prove the person may see: a model or field missing from
  `/api/meta` renders "No access" and nothing is requested.
- Tile filters are text: `field operator value; …`. Operators: `=`, `!=`, `~`, `>`, `>=`, `<`, `<=`,
  `in` (values separated by `|`), `is empty`, `is set`. A JSON object of exact values (the package
  documentation's form) is read as equality on each key.

## The record on small screens, language changes, failures

- Up to 1,024 px the record is a sheet over the list: a right-side panel (inline end) on a tablet, the
  whole screen on a phone (up to 640 px) with a "Back to list" button. It is a dialog (`role="dialog"`,
  `aria-modal`): focus is trapped and returned to the opener, Esc closes it, the page behind does not
  scroll (`html.sheet-open`), and the list stays mounted underneath, so its selection and scroll are
  there when it closes. Wider than that the panel sits beside the list and is not modal.
- Changing the language does not remount the app (D6): `I18nProvider` clones the root element so the
  whole tree renders again with the new text, and every component keeps its state: the route, an open
  record panel, a dialog, a draft. Two things keep old text through a change, and the rules follow
  from them. A `useMemo` (or a `memo` component) that translates, sorts or formats keeps its value
  unless `lang` (text) or `locale` (collation, number parsing; from `useI18n()`, or `d.lang` of the data
  context) is among its dependencies. And state must not hold a finished translated sentence: keep the
  raw message or a code and word it when drawing (`RecordPanel` keeps the server's own words and calls
  `serverMessage` at render). `web/apps/backoffice/src/i18nDeps.test.ts` guards the first. It reads
  every source file of the apps and of `packages/ui`, works out from the call graph which functions
  translate, sort or format (those that use a reader of the i18n store, directly or through other
  functions: called, handed on as in `.sort(byPosition)`, used as a JSX tag, or a member such as
  `d.recordLabel`), and fails a `useMemo` or `memo` that uses one of them while its dependencies name
  neither `lang` nor `locale` (a deliberate exception carries `// i18n-memo-ok: <why>`). It does not
  read `useCallback`, `useEffect`, a `useState` initialiser, a ref, or state that holds translated text;
  docs/ISSUES.md D8 to D10 and D14 list what is known to slip through. A query that the server answers
  in the collation of the locale (a sorted `useServerRows`, the sorted Kanban columns) asks again when
  the locale changes; one without a sort does not, because the request carries no language.
- A dropped connection never escapes as an unhandled rejection. Every command answers an error reply
  ("Could not reach the server"); reads (get, list, search, meta) retry twice with a short backoff, writes
  never; a page-wide banner says the app is offline or the server cannot be reached and clears itself
  (and reloads what is on screen) when it answers again. A 500 or an HTML error page becomes "The server
  had a problem (status 500)". A first load that fails shows "Could not load your workspace" with Try
  again. A screen that crashes while rendering shows a recovery screen (Try again, Reload, back to the
  apps) and logs the error to the console only; nothing is sent anywhere. Big routes load on demand and
  show a skeleton meanwhile.
