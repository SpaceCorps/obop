# 14 - Services: Projects, Timesheets, Helpdesk, Field Service, Planning, Appointments (plus Fleet, Repairs, Contracts, Portal)

## Scope & method

Research date is 2026-10-01. Primary sources were: the Odoo 20.0 Community source on GitHub (read as raw files, Odoo Community is LGPL-3), the Odoo 19/20 documentation at odoo.com, the source repositories of OpenProject, Plane, Taiga, Zammad, Chatwoot, Cal.diy and the OCA (Odoo Community Association) modules, vendor pricing pages (Odoo, Jira, Jira Service Management, Zendesk, Freshdesk, Zammad, Chatwoot, Linear, Plane, OpenProject), RFC Editor pages, Google and Microsoft calendar API docs, and crates.io / GitHub release APIs. Tags: **[V]** verified against a live source today, **[I]** inference from verified facts, **[G]** guess or remembered fact not re-verified. Secondary sources were not used for sentiment: none was gathered (see gaps). Odoo Enterprise source is not public, so Enterprise behaviour is taken from documentation only and must never be copied; this document describes behaviour and data shape, not code.

---

## 1. Landscape snapshot (as of 2026-10-01)

| Product | Role | License / model | Current version [V] | Notes |
|---|---|---|---|---|
| Odoo | functional reference | Community LGPL-3 (https://github.com/odoo/odoo/blob/19.0/LICENSE); Enterprise proprietary (Odoo Enterprise Edition License v1.0, "OEEL-1", https://www.odoo.com/documentation/19.0/legal/licenses.html) | 20.0 "September 2026" (https://www.odoo.com/page/release-notes); branch `20.0` is FINAL (`version_info = (20, 0, 0, FINAL, 0, '')`), `master` is 20.1 alpha (`(20, 1, 0, ALPHA, 1, '')`) (https://github.com/odoo/odoo/blob/20.0/odoo/release.py); docs also list 19.4 (saas) | Per-user pricing: Standard EUR 19.90/user/mo yearly (24.90 monthly), Custom EUR 35.90 / 44.90, One App Free (https://www.odoo.com/pricing-plan). The page presents these as a discount "valid for 12 months, for initial users ordered"; the struck-through list prices are Standard 24.90 yearly / 31.10 monthly and Custom 44.90 yearly / 56.90 monthly; Light User 7.90 (page HTML, checked 2026-10-01) |
| OpenProject | project mgmt (Jira-alternative) | GPL-3.0, open core (https://github.com/opf/openproject) | v17.9.0, 2026-09-30 (https://www.openproject.org/docs/release-notes/17-9-0/) | Ruby on Rails (Gemfile `rails ~> 8.1.3`) [V]; Gantt, boards, time and cost, custom workflows all in free Community (https://www.openproject.org/pricing/) |
| Plane | project mgmt (Linear/Jira-alternative) | AGPL-3.0 repo, paid tiers add features (https://github.com/makeplane/plane) | v1.4.2, 2026-08-23 | Django API + React Router web; repo already uses Oxc tooling (`.oxlintrc.json`, `.oxfmtrc.json`) [V, repo tree] |
| Taiga | agile PM | back MPL-2.0, front AGPL-3.0 (https://github.com/taigaio) | back tag 6.10.2, front package 6.10.3 | Front is AngularJS 1.5.10 and back is Django 3.2.25, both end-of-life stacks [V, package.json / requirements.txt]; still receiving commits (last 2026-09-28) - maintenance mode [I] |
| Zammad | helpdesk | AGPL-3.0, no feature-gated edition; paid = hosting/support (https://github.com/zammad/zammad) | 7.2.0 stable, 7.3.0-alpha in dev (tags) | Rails; Zammad Foundation owns the code [V, README] |
| Chatwoot | omnichannel inbox/helpdesk | MIT except `enterprise/` dir under a paid license (https://github.com/chatwoot/chatwoot/blob/develop/LICENSE) | v4.18.0, 2026-09-18 | SLA, custom roles, audit log, agent capacity, Captain AI live in `enterprise/` [V, directory listing] |
| Cal.com | scheduling | **Closed source since 2026-04-14**; public repo is now `calcom/cal.diy` (MIT, enterprise code removed) (https://cal.com/blog/cal-com-goes-closed-source-why) | Cal.diy v6.2.0 (2026-03-01) tag; default-branch commits continue to 2026-09-20 (last push to any branch 2026-09-26) | Cal.diy README says "strictly recommended for personal, non-production use" [V] |
| Linear | issue tracker | proprietary SaaS | n/a | Free; Basic USD 10; Business USD 16 per user/mo (yearly); Issue SLAs only Business+ (https://linear.app/pricing, https://linear.app/docs/sla) |
| Jira / JSM | issue tracker / service desk | proprietary SaaS | n/a | Jira Standard USD 7.91, Premium 14.54 per user/mo; cross-project dependencies only Premium+ (https://www.atlassian.com/software/jira/pricing). JSM: Free 3 agents, Standard USD 20, Premium USD 51.42 per agent/mo (https://www.atlassian.com/collections/service/pricing?bundle=jira-service-management). Billing-toggle state on the pages not confirmed (the pages' JSON-LD lists 7.91 / 14.54 / 20.00 / 51.42 as USD per user per month with no annual/monthly label; the JSM page also shows "Switch to Annual billing above", so these may be the monthly-billing rates) (unverified). Jira Free is 10 users, JSM Free is 3 agents [V]. |
| Zendesk | helpdesk | proprietary SaaS | n/a | Support Team USD 19/25, Suite Team 55/69, Suite Professional 115/149 (annual/monthly); SLAs and CSAT only from Suite Professional; AI resolutions USD 1.50 committed / 2.00 pay-as-you-go (https://www.zendesk.com/pricing/) |
| Freshdesk | helpdesk | proprietary SaaS | n/a | Growth USD 19, Pro 55, Enterprise 89 per agent/mo annual; multiple SLA policies from Pro (https://www.freshworks.com/freshdesk/pricing/) |
| OCA (Odoo Community Association) | open modules filling Odoo's Community gaps | AGPL-3.0 | branches 11.0-20.0 exist for `field-service`; the `20.0` branch is scaffolding only (no addons yet), repo pushed 2026-10-01 | `OCA/field-service` (33 `fieldservice_*` modules on 19.0, 37 on 18.0), `OCA/helpdesk` (`helpdesk_mgmt*`), `OCA/project`, `OCA/timesheet`, `OCA/contract` (19.0 active; 20.0 branch is scaffolding only) [V, GitHub API] |

Pricing anchors for positioning: self-hosted Zammad has no per-agent fee (support packages EUR 2,999 / 5,999 / 9,999 per year, https://zammad.com/en/pricing); Chatwoot cloud is USD 0/19/39/99 per agent (https://www.chatwoot.com/pricing).

**Rust gap [V/I]:** GitHub searches for Rust helpdesk, project-management, scheduling and field-service projects return no business services suite. The largest hits are not comparable products: `BloopAI/vibe-kanban` (~28k stars, Apache-2.0, a kanban for orchestrating AI coding agents, Rust + TypeScript) and `macro-inc/macro` (~4.5k stars, AGPL-3.0, a team workspace with email/chat/docs/tasks/CRM, Rust + TypeScript); next are TUI kanban tools (`kanban-rs/kanban` ~172 stars), and `open-deltat/deltat` is an 8-star AGPL scheduling database (searches run 2026-10-01). There is no Rust incumbent to fork or learn from for helpdesk, field service or timesheet-to-invoice billing; obop builds this domain from scratch, using Odoo and the Python/Ruby/TypeScript projects as functional references.

---

## 2. What Odoo ships where (Community vs Enterprise)

Determined by listing `addons/` on the 20.0 branch (https://github.com/odoo/odoo/tree/20.0/addons). Presence in the public repo is [V]; "Enterprise/proprietary" is an inference from absence plus the editions page listing these apps (https://www.odoo.com/page/editions) [I].

| Service area | Community (LGPL-3, in repo) [V] | Not in Community repo (Enterprise, proprietary) [I] |
|---|---|---|
| Projects | `project`, `project_todo`, `project_account`, `project_purchase`, `project_stock`, `project_mrp`, `project_sms`, `project_hr_skills`, `website_project`, `sale_project`, `crm_sale_project` | Gantt (`web_gantt`, `project_enterprise`), forecast, task auto-scheduling UI |
| Timesheets | `hr_timesheet`, `hr_timesheet_attendance`, `sale_timesheet`, `sale_timesheet_margin`, `project_timesheet_holidays`, `website_timesheet` (the separate `hr_hourly_cost` module exists on 19.0 but is absent from the 20.0 tree; `hourly_cost` is now a field on `hr.employee` in `hr`) | `timesheet_grid` (grid entry, timer, validation/approval, leaderboard) |
| Helpdesk | none (`rating`, `portal_rating`, `im_livechat`, `website_forum`, `website_slides`, `survey` are generic Community pieces) | `helpdesk` and all its SLA / after-sales glue |
| Field service | none (`website_google_map`, `base_geolocalize` exist) | `industry_fsm` and worksheet reports (19.0 and earlier); the Odoo 20 release notes say the Field Service app "has been discontinued" and its features are merged into Planning (https://www.odoo.com/odoo-20-release-notes) |
| Planning | `resource` (calendars, attendances, leaves), `hr_holidays` | `planning` (shifts, roles, open shifts, auto-plan) |
| Appointments | `calendar`, `google_calendar`, `microsoft_calendar` | `appointment` (public booking pages, resources, capacity) |
| Fleet | `fleet`, `fleet_maintenance` (new in 20.0), `hr_fleet`, `account_fleet`, `stock_fleet` | none |
| Repairs / maintenance | `repair`, `mrp_repair`, `pos_repair`, `purchase_repair`, `maintenance`, `hr_maintenance` | none |
| Contracts / recurring | n/a | `sale_subscription` (see `11-crm-sales-subscriptions.md`); OCA `contract` fills the gap |
| Portal | `portal`, `portal_rating`, `portal_discuss`, project sharing | helpdesk portal pages |
| Knowledge base | `website_slides`, `website_forum` | `knowledge` |

Takeaway: Odoo Community gives projects, basic timesheets, fleet, repairs and portal. The **money-making service apps (helpdesk, planning, field service, appointments, Gantt, timesheet grid/approval) are Enterprise-only**. Those are exactly the gaps the OCA and the free competitors (OpenProject CE Gantt, Zammad, Chatwoot) fill, and the obvious open-source differentiation for obop.

---

## 3. Project management

### 3.1 Odoo 20 data model [V, https://github.com/odoo/odoo/tree/20.0/addons/project/models]

| Entity (model) | Key fields / behaviour |
|---|---|
| `project.project` | name, description, `partner_id` (customer), `user_id` (manager), `date_start`, `date` (expiration), `privacy_visibility` (invited / internal / portal), `allowed_internal_user_ids`, `account_id` (analytic account), `resource_calendar_id`, `type_ids` (task stages, M2M), `stage_id` (optional project-level stages with rotting-days), feature toggles `allow_task_dependencies`, `allow_milestones`, `allow_recurring_tasks`, `is_template`, `label_tasks`, `alias_id` (email-in), `favorite_user_ids`, `task_properties_definition` (custom fields), `last_update_id` / `last_update_status` |
| `project.task` | name, description (HTML), `priority` (0-3: Low/Medium/High/Urgent), `stage_id`, **`state`** enum, `project_id`, `user_ids` (M2M assignees), `role_ids`, `partner_id`, `email_from`, `allocated_hours`, `date_deadline`, `date_end`, `date_assign`, `parent_id` / `child_ids` (sub-tasks), `milestone_id`, `depend_on_ids` / `dependent_ids` (M2M table `task_dependencies_rel`), recurrence (`recurrence_id`, `repeat_interval`, `repeat_unit`, `repeat_type`, `repeat_until`), `personal_stage_type_id` (per-user private kanban), `tag_ids`, `is_template`, stored working-days-to-assign/close |
| `project.task.type` (stage) | per-project, M2M to projects; fold flag; rating template; mail/SMS template on entry |
| `project.milestone` | name, deadline, reached flag, optional `sale_line_id` + `quantity_percentage` (via `sale_project`) |
| `project.update` | status post (on track / at risk / off track / on hold / done) with text, feeds project dashboard |
| `project.role`, `project.tags`, `project.collaborator`, `project.task.recurrence`, `project.task.stage.personal` | roles for templates, labels, portal/external access (`access_mode`: read / edit / advanced_edit), recurrence, personal stages |
| Reports | `project.task.burndown.chart.report` (non-materialised, `_auto = False`), `project.time.by.stage.report`, `project.report` |

Important design points:

- **Stage vs state split [V]**: stage is a user-configured kanban column (Odoo ships none by default); state is a fixed enum: `01_in_progress`, `02_changes_requested`, `03_approved`, `04_waiting_normal`, `1_done`, `1_canceled` (`CLOSED_STATES`). "Waiting" is set automatically while blockers are open; Done and Cancelled close the task independent of stage (https://www.odoo.com/documentation/19.0/applications/services/project/tasks/task_stages_statuses.html). Plane uses the same idea with fixed state *groups* (backlog, unstarted, started, completed, cancelled, triage) over custom states (https://github.com/makeplane/plane/blob/preview/apps/api/plane/db/models/state.py). OpenProject uses custom statuses with an "is closed" flag plus per-type, per-role transition matrices with "author" and "assignee" extra transitions (https://www.openproject.org/docs/system-admin-guide/manage-work-packages/work-package-workflows/).
- **Dependencies [V]**: Odoo only has "blocked by"; a blocked task shows state Waiting and cannot move to In Progress until every predecessor is Approved/Cancelled/Done (https://www.odoo.com/documentation/19.0/applications/services/project/tasks/task_dependencies.html). OpenProject is far richer: relates / duplicates / blocks / includes / requires / parent-child, plus precedes/follows with **lag in working days (negative allowed)** and a per-work-package **automatic vs manual scheduling mode** (https://www.openproject.org/docs/user-guide/work-packages/work-package-relations-hierarchies/). Jira limits dependency management to a single project until Premium (https://www.atlassian.com/software/jira/pricing); Plane gates "Dependencies in Timeline" at Pro (https://plane.so/pricing).
- **Gantt**: Enterprise in Odoo, free in OpenProject Community. Milestones render as diamonds; Odoo 20 Gantt gained starting/ending buffers and drag-to-schedule from a side panel (the latter is enabled per Gantt view via Studio) (https://www.odoo.com/odoo-20-release-notes). Project templates compute a start-date "delta" per task so a template instantiated at a new start date shifts all planned dates, with end dates from a scheduling algorithm honouring dependencies, allocated time, working schedules, time off and public holidays (https://www.odoo.com/documentation/19.0/applications/services/project/project_management/project_templates.html).
- **Burndown [V]**: Odoo 20 derives burndown from a per-task JSONB `duration_tracking` (minutes spent per stage) unrolled in SQL by a CTE (https://github.com/odoo/odoo/blob/20.0/addons/project/report/project_task_burndown_chart_report.py), and the view must be grouped by date and stage. This only works with "stage" history; it is an inferred weakness that estimates (allocated hours) are not re-snapshotted [I]. Plane stores `Cycle.progress_snapshot` JSON; Linear computes capacity from velocity of the last three cycles (https://linear.app/docs/use-cycles).
- **Recurring tasks [V]**: next occurrence is created when the task becomes Done/Cancelled, copying name, description, project, assignees, customer, tags, but not milestones, timesheets, activities or sub-tasks (https://www.odoo.com/documentation/19.0/applications/services/project/tasks/recurring_tasks.html).
- **Sprints/cycles**: Linear cycles are 1-8 weeks with cooldown, auto-add, rollover and capacity dial (https://linear.app/docs/use-cycles). Plane has Cycles (sprints), Modules (epics) and Intake (triage: pending / rejected / snoozed / accepted / duplicate, from `intake.py`). Odoo Project has no sprint object [I: none in the models directory].
- **Odoo 20 Project deltas [V, https://www.odoo.com/odoo-20-release-notes]**: team members can be assigned to project roles per project; customers can get portal access as *collaborators* without being followers; tasks can be assigned to portal users who follow a project; projects can be created from CRM opportunities; a task schedule can be printed from the Gantt view; task chatter changes show in the portal. The release notes also state that the profitability report in the project dashboard "has been removed" in favour of new dedicated budget and margin reports, while the 20.0 docs page for project profitability still describes the dashboard (unresolved which is current).

### 3.2 Comparison

| Capability | Odoo (CE / EE) | OpenProject CE | Plane | Linear | Jira | Taiga |
|---|---|---|---|---|---|---|
| Gantt / timeline | EE | yes (PDF export, baseline beyond "yesterday" are EE) | yes, deps Pro+ | timeline/roadmap [G] | Plans (Premium) [I] | no native Gantt [G] |
| Dependencies with lag | blocked-by only | yes, lag, auto/manual | Pro+ | relations only [G] | cross-project Premium | no [G] |
| Sprints | no | Scrum backlogs (sprint sharing EE) | Cycles | Cycles | yes | yes (Scrum) [G] |
| Time tracking | yes (timesheets) | yes, costs, budgets | worklogs Pro+, approvals Business+ | none built in [G] | plugin [G] | no [G] |
| Billing link | yes (sale_timesheet) | cost reports only | no | no | no | no |
| Workflows | stage + fixed state | types x roles matrix | Business+ single, Enterprise multiple + approvals | team workflows [G] | yes | no [G] |

Sources: OpenProject pricing/community matrix (https://www.openproject.org/pricing/), Plane pricing (https://plane.so/pricing), Jira pricing. Cells marked [G] are from memory.

---

## 4. Timesheets and billing (time and material, fixed price, milestones)

### 4.1 Data model (Odoo 20) [V, https://github.com/odoo/odoo/tree/20.0/addons/hr_timesheet, /sale_timesheet, /sale_project]

- **A timesheet is an `account.analytic.line`** extended with `task_id`, `project_id`, `employee_id`, `department_id`, `manager_id`, `encoding_uom_id` (hours or days). The same table carries accounting analytic lines, so profitability and reporting unify costs and revenues per analytic account (`project.account_id`).
- `sale_timesheet` adds `so_line` (sales order item) on the analytic line, `is_so_line_edited`, and `project.project` fields `pricing_type` (`task_rate` / `fixed_rate` "project rate" / `employee_rate`), `sale_line_employee_ids` (`project.sale.line.employee.map`: employee -> SO line, with unit price and per-project cost override), `timesheet_product_id`, `billing_type` (not_billable / billed manually).
- `hourly_cost` (a Monetary field on `hr.employee` in the Community `hr` module on 20.0; it lived in a separate `hr_hourly_cost` module on 19.0) stores an employee hourly cost used for cost-side profitability; `hr_timesheet` reads it through `_hourly_cost()` [V, 20.0 source].
- Delivered quantity: `sale.order.line.qty_delivered_method = 'timesheet'` sums timesheets against the SO line; the code comments "Hook for validated timesheet in additional module" - the Enterprise `timesheet_grid` restricts delivered quantity to validated entries [I].

### 4.2 The four service invoicing policies [V]

From `sale_project` / `sale_timesheet` `product.template`:

| `service_policy` | Meaning | Invoice policy / service type |
|---|---|---|
| `ordered_prepaid` | Prepaid / fixed price | order / manual (or timesheet to track prepaid hours) |
| `delivered_manual` | Based on delivery, manually set | delivery / manual |
| `delivered_timesheet` | Time and material | delivery / timesheet |
| `delivered_milestones` | Milestone billing (needs milestones feature) | delivery / milestones |

Combined with `service_tracking` on the product: `no`, `task_global_project` (task in a shared project), `task_in_project` (project + task per order), `project_only`. On SO confirmation, Odoo creates the project/task(s) (optionally from a `project_template_id` / `task_template_id`), copying the SO line onto the project (`sale_line_id`) and tasks. Milestones carry `sale_line_id` + `quantity_percentage`: when a milestone is reached, that percentage of the ordered quantity is marked delivered, which makes the SO line invoiceable (https://www.odoo.com/documentation/19.0/applications/services/project/project_management/project_milestones.html).

### 4.3 Timesheet entry, validation and profitability

- UX [V, https://www.odoo.com/documentation/19.0/applications/services/timesheets.html]: grid per project per day, suggestions (italic) from assignments/deadlines/history, start/stop timer, colour-coded totals vs working hours, four access levels (none / own / all / admin). Grid, timer, validation and leaderboard are Enterprise.
- Profitability dashboard [V, https://www.odoo.com/documentation/19.0/applications/services/project/project_management/project_profitability.html; Odoo 20 release notes report it removed from the project dashboard in favour of dedicated budget/margin reports, see section 3.1]: revenues (sold / delivered / invoiced, split by invoicing policy, materials, invoices, reinvoiced expenses) vs costs (timesheet cost from employee hourly cost, purchase orders once billed, stock-move materials needing analytic costs). Expected -> to invoice/to bill -> invoiced/billed.
- Odoo 20 adds a "Billing targets vs billable time" timesheet dashboard (https://www.odoo.com/odoo-20-release-notes).
- Plane gates worklogs (Pro, historical) and approvals (Business); OpenProject has time and cost tracking free with "required exact time tracking" EE (Professional+) (https://www.openproject.org/pricing/).

### 4.4 Sales order -> project -> timesheet -> invoice (link to sales and accounting)

1. Sales order line with service product (policy `delivered_timesheet`, tracking `task_in_project`) is confirmed -> project (from template) + task created, `sale_line_id` set; billable project requires `allow_billable`.
2. Employees log `account.analytic.line` with `so_line`; default SO line comes from task, else employee-rate map, else project default.
3. `qty_delivered` on the SO line recomputes from timesheets; the SO invoice status moves to "to invoice".
4. Invoice creation produces `account.move` lines linked to the SO lines; the invoice exposes `timesheet_ids` (computed) and total duration so customers can see billed time.
5. Vendor bills, expenses and purchases tagged with the same analytic account feed the project P&L. Fixed-price orders invoice up-front/at milestones; downpayments supported via the sales flow (see `11-crm-sales-subscriptions.md`).

OpenProject, Jira, Linear and Plane have **no** sales-order or invoice link; this integration is a defining advantage of the Odoo model and the reason obop should model time and analytic accounting as one table.

---

## 5. Helpdesk

### 5.1 Odoo Helpdesk (Enterprise; documentation only) [V, https://www.odoo.com/documentation/19.0/applications/services/helpdesk.html]

Entities: Helpdesk team (name, company, visibility - private / all internal / public with portal followers; automatic assignment by equal count, by equal open count, or expertise via tags; time-off-aware), ticket (pipeline per team with stages), ticket tag, SLA policy, stage with rating email template, website form, email alias per team.

- **SLA** (https://www.odoo.com/documentation/19.0/applications/services/helpdesk/overview/sla.html): policy criteria = team (one), priority (0-3 stars), tags, customers, service (sales order item). Target = "reach stage X within N working hours", with *excluded stages* that pause the clock; working hours default to the company calendar but can be overridden; deadline is computed from ticket creation; tag turns green when hit. Pivot/graph/cohort reporting.
- **Channels**: email alias, website form, live chat (`/ticket` command); in Odoo 20 the Gmail/Outlook mail plugins can search a contact, task or helpdesk ticket and log an email in its chatter (logging to an existing record, not ticket creation). Other Odoo 20 helpdesk additions: AI similar-ticket detection, a reminder email before auto-closing, and a filter for unanswered tickets (https://www.odoo.com/odoo-20-release-notes).
- **CSAT** (https://www.odoo.com/documentation/19.0/applications/services/helpdesk/overview/ratings.html): a per-stage email template fires when a ticket enters a (folded/closed) stage; 3-point scale (satisfied / okay / dissatisfied); page takes comments; appears in chatter and in a Customer Ratings report.
- **Help Center**: Knowledge articles, Forum and eLearning linked to the team; portal visibility must be "public".
- **After-sales**: refund (credit note wizard tied to the invoice), coupon, return (reverse transfer), repair (repair order) and field service task, all created from the ticket (https://www.odoo.com/documentation/19.0/applications/services/helpdesk/advanced/after_sales.html).
- **Time and billing**: tickets accept timesheets; SLAs can target a sales order item (prepaid support hours); ticket -> CRM lead conversion; duplicate merge needs Data Cleaning.

### 5.2 Competing data models (from source)

| Concept | Zammad (Rails) [V] | Chatwoot (Rails) [V] |
|---|---|---|
| Ticket | `Ticket` with group, owner, customer, organization, state (type: new/open/pending reminder/pending action/closed, "ignore escalation"), priority (custom), number, title, `first_response_*`, `close_*`, `update_*` escalation timestamps | `Conversation`: status enum `open 0 / resolved 1 / pending 2 / snoozed 3`, priority enum `low..urgent`, `first_reply_created_at`, `waiting_since`, `snoozed_until` |
| Grouping | Groups, overviews (saved queues), organizations | Inboxes (one per channel), teams, labels |
| SLA | `Sla` + `Calendar`: conditions selector, **first response**, **update time** (two modes), **solution time**; frozen states pending-close/reminder/closed; agents warned ~15 min before escalation (https://admin-docs.zammad.org/en/latest/manage/slas.html) | `SlaPolicy` (`first_response_time_threshold`, `next_response_time_threshold`, `resolution_time_threshold`, `only_during_business_hours`) + `AppliedSla.sla_status`: active / hit / missed / active_with_misses + `SlaEvent` - **in `enterprise/` (paid)** |
| Automation | triggers, schedulers (jobs), macros, Core Workflows, text modules, checklists | automation rules, macros, canned responses |
| Knowledge base | `KnowledgeBase` (built in, multilingual on Plus hosted plan) | `Portal` / `Category` / `Folder` / `Article` (open source) |
| Time | `Ticket::TimeAccounting` (type/activity) | none |
| CSAT | n/a in models list | `CsatSurveyResponse` (open source) |
| Custom fields | Object Manager (admin-defined attributes on ticket, user, org, group) | `CustomAttributeDefinition` |
| Channels | email, web form, chat, phone (Plus) (unverified), SMS, Telegram, Facebook, WhatsApp | API, Email, Facebook, Instagram, Line, SMS, Telegram, TikTok, Twilio SMS, Web widget, WhatsApp |

Also: Zendesk gates SLAs/CSAT at Suite Professional; Freshdesk puts multiple SLA policies, round-robin and load-balanced assignment at Pro and skill-based assignment at Enterprise; Zammad's hosted pricing lists SLAs from the Professional v2 plan upward (EUR 16/18 per agent/mo annual/monthly; Starter v2 EUR 7/9, Plus v2 EUR 25/27; https://zammad.com/en/pricing), while the self-hosted code has everything. JSM structures SLAs as goals with start/pause/stop conditions, calendars and JQL [page titles verified, semantics not retrieved; G].

### 5.3 Workflow essentials to clone

Ticket lifecycle: create (email/form/chat/API/portal) -> auto-assign (round-robin, load, expertise tag, skip employees on leave) -> SLA applied -> stages -> solved/closed stage triggers rating email -> optional follow-ups (refund, return, repair, FSM task, timesheet, invoice).

---

## 6. Field service

### 6.1 Odoo Field Service (Enterprise) [V, https://www.odoo.com/documentation/19.0/applications/services/field_service.html]

- **Odoo 20 change [V, https://www.odoo.com/odoo-20-release-notes ; https://www.odoo.com/documentation/20.0/applications/services/planning/field_service.html]**: the standalone Field Service app is discontinued and merged into Planning. An intervention is now a Planning *shift* (created from Planning, a sales order or a helpdesk ticket) with `Customer`, `Sales Order`, `Worksheet` and `Under warranty` fields, Sign In / Complete actions, optional geolocation logged in the chatter, materials and time billed from the shift, and a customer report generated for signature. Worksheet templates now use property fields instead of Studio fields. Other 20 additions: live map of technicians, ordered vs optimised routing in the map view, distance-based travel fees, website request form, product barcode scanning, equipment covered by a Subscriptions maintenance contract, customer ratings, and customer locations in Auto Plan. The bullets below describe the 19.0 documentation (project-and-task based) and are kept as the behavioural baseline.
- A *field service project* (19.0) holds tasks created manually, from a confirmed sales order service product, or from a helpdesk ticket.
- **Worksheets**: per-task-type templates built in Studio (text, checkboxes, lists, HTML, files, images); "Worksheet Complete" button; requires Studio, "which may impact your price plan". A customer report for signature is generated when an intervention is completed [V, Odoo 20 docs above]; the 19.0 module layout for it was not re-verified.
- **Products on task**: product catalog on the task, "Time and Material Invoicing" setting, products added appear on the invoice and decrement stock; technician default warehouse (van stock) applies.
- **Map and itinerary**: static map with pins for today's tasks; routes between tasks ordered by planned date need the Mapbox integration; "View in Google Maps" opens directions (https://www.odoo.com/documentation/19.0/applications/services/field_service/planning_itinerary.html).
- **Dispatch**: via Planning (Gantt by employee/role, open shifts, auto-plan).

### 6.2 Open-source prior art: OCA `field-service` (AGPL-3.0) [V]

Models (https://github.com/OCA/field-service/tree/19.0/fieldservice/models): `fsm.order` (stage, priority, tags, team, location, person(s), request early/late, scheduled start/duration/end, actual start/end, territory/branch/district/region, template, categories, equipment, order type), `fsm.location`, `fsm.person`, `fsm.equipment`, `fsm.stage`, `fsm.team`, `fsm.template`, `fsm.category`, `fsm.tag`, `fsm.order.type`. Add-on modules exist for sale, stock, timesheet, agreement, recurring, route, skill, portal, vehicle, equipment warranty, Google map, repair, sign. Strongest public evidence of the entity set field service actually needs: **location/site and equipment are first-class**, not just a task with a customer address.

### 6.3 Routing / maps stack options (all with live status)

| Component | License | Latest [V] | Fit |
|---|---|---|---|
| OSRM | BSD-2-Clause | v26.10.0 (2026-10-01) | fast shortest-path/matrix; C++ service |
| Valhalla | MIT (COPYING file) | 3.9.0 (2026-09-19) | multimodal, time-dependent; Rust bindings crate `valhalla` 0.6.46 exist |
| GraphHopper | Apache-2.0 | 11.1 (2026-09-29) | Java; includes routing optimisation |
| VROOM | BSD-2-Clause | v1.15.0 (2026-03-12) | vehicle-routing (VRP) solver on top of OSRM/Valhalla |
| Google OR-Tools | Apache-2.0 | v9.15 (2026-01-12) | general VRP/scheduling solver |
| MapLibre GL JS | BSD-3-Clause (LICENSE.txt) | v6.11.2 (2026-09-24) | open fork of Mapbox GL; the right front-end map lib |
| Photon geocoder | Apache-2.0 | 1.3.0 (2026-08-07) | OSM geocoding |

OSM's public tile servers (`tile.openstreetmap.org`) are **unsuitable as a production dependency**: no SLA, can be blocked, bulk/offline prohibited, requires identifying User-Agent, Referer and attribution (https://operations.osmfoundation.org/policies/tiles/). Self-host vector tiles or use a commercial provider.

---

## 7. Planning / resource scheduling

Odoo Planning (Enterprise) [V, https://www.odoo.com/documentation/19.0/applications/services/planning.html]: resources are employees **or materials** (equipment), each with working hours (blank = flexible), roles (M2M), and a default role. A *shift* has resource (empty = **open shift**), role, start/end, allocated hours or percentage, project / task / sales order item, template, repeat (recurring shifts generated six months ahead), employee note, published/unpublished (unpublished drawn striped; "Publish & Save" emails the employee). Gantt view groups by resource/role/project/SO; drag/resize/split/duplicate; time off greyed; **Auto Plan** assigns open shifts by role and availability (default role first; no double-booking; time off, public holidays, working hours and active-contract period respected; not usable for flexible-hours employees or employees without roles) [V, 19.0 docs]. Employees can "Ask to switch" a published shift (same-role colleagues can take it with "I take it"), or, when "Allow Unassignment" is on (mutually exclusive with switching), click "I am unavailable" to turn the shift back into an open shift [V, 19.0 docs]; employees self-assigning arbitrary open shifts was not confirmed (unverified). Sales integration: service products link to roles, so a confirmed order creates shifts for that role; Project integration: tasks are planned on resources (Gantt). Odoo 20 planning additions [V, release notes]: link a shift to a task, shift priority (higher-priority shifts auto-planned first), multiple resources per shift, materials assigned to employees, send schedules via WhatsApp, Mapbox travel times in Gantt, and removal of the Planning-Payroll integration and work entries.

OpenProject puts **team planner** in EE Basic+ (EUR 5.95 per user/mo, minimum 25 users) and **resource management** in EE Premium+ (EUR 15.95, minimum 100 users; Professional is EUR 10.95) (https://www.openproject.org/pricing/). Linear and Plane plan by team/cycle, not by named hourly capacity.

Shared primitive: `resource.calendar`, `resource.calendar.attendance`, `resource.calendar.leaves`, `resource.resource` (Community `resource` module [V]) - used by SLA deadlines, project scheduling, timesheet expectations, planning, appointments and time off. This is the single most reused foundation of the services domain.

---

## 8. Appointments and booking with calendar sync

### 8.1 Odoo Appointments (Enterprise) [V, https://www.odoo.com/documentation/19.0/applications/productivity/calendar.html, https://www.odoo.com/odoo-20-release-notes]

Appointment types with: availability (weekly/flexible), users or *resources* (capacity, auto-confirm), questions, intro/confirmation messages, reminders, invitations, minimum booking notice, cancellation window, timezone, optional CRM opportunity creation, optional up-front payment via payment providers, website publishing. Odoo 20 adds per-slot booking limits, manual max capacity, guest count aware resource assignment, accessory products at checkout, rescheduling "in just a few clicks", post-appointment feedback, calendar views and Gantt attendance. Backend sync: Google and Outlook calendar connectors in Community (`google_calendar`, `microsoft_calendar`).

### 8.2 Cal.diy / Cal.com data model (reference; Prisma schema) [V, https://github.com/calcom/cal.diy/blob/main/packages/prisma/schema.prisma]

`EventType` (bookable type) - `Host` (user x event type, isFixed, priority, weight, optional own `Schedule`) - `Schedule` (name, timezone) - `Availability` (days[], start/end time, or a specific `date` override) - `Booking` (uid, idempotencyKey, start/end, status `PENDING/ACCEPTED/REJECTED/CANCELLED/AWAITING_HOST`, `iCalUID`, `iCalSequence`, paid flag, rescheduling and cancellation reasons, metadata JSON, rating, noShowHost, recurringEventId) - `Attendee` - `BookingReference` (per external calendar/video event id) - `Credential` (OAuth tokens per integration) - `DestinationCalendar` (where to write bookings) - `SelectedCalendar` (calendars read for busy times, with Google watch-channel fields: channelId, resourceId, expiration) - `Payment` - `Webhook`. `SchedulingType` is `ROUND_ROBIN`, `COLLECTIVE`, `MANAGED`. Plus Teams/Memberships, OutOfOfficeEntry, seats (`BookingSeat`); the Workflow and routing-form models are not present in the Cal.diy schema (removed with the enterprise features), so they are Cal.com-only.

**Licensing lesson**: Cal.com was AGPL open-core for five years; on 2026-04-14 it moved its production code to a private repo, citing AI-assisted vulnerability scanning of public code, and left `cal.diy` (MIT, Teams/Orgs/Workflows/SSO/Insights removed) behind (https://cal.com/blog/cal-com-goes-closed-source-why). Open-core dependencies on a single vendor are fragile; obop should not embed Cal.com.

### 8.3 Calendar sync mechanics (external APIs) [V]

- **Google Calendar**: push "watch" channels per calendar; `expiration` is optional and bounded by Google's internal limits (the page documents no fixed maximum; the more restrictive value wins), there is **no automatic renewal** (create a new channel with a new id before expiry); notifications carry **no body** (headers only, states `sync`/`exists`/`not_exists`), delivery is not guaranteed, so a periodic sweep is required; callback must be HTTPS with a valid cert (https://developers.google.com/workspace/calendar/api/guides/push). Incremental sync via `syncToken`, `pageToken` paging, deleted items returned, and **410 Gone requires wiping local state and a full resync** (https://developers.google.com/workspace/calendar/api/guides/sync).
- **Microsoft Graph**: Outlook `event` subscriptions last at most **10,080 minutes (under 7 days)**, 1,440 minutes with resource data; 1,000 active subscriptions per mailbox across all apps; the latency table gives the `calendar` resource under 1 minute average, max 3 minutes, but lists `event` latency as "Unknown" (https://learn.microsoft.com/en-us/graph/change-notifications-overview). Lifecycle notifications warn about missed events.
- **Standards**: iCalendar RFC 5545 (Proposed Standard, 2009; updated by 5546 iTIP, 6868, 7529, 7953, 7986, 9073, 9074, 9253 - https://www.rfc-editor.org/info/rfc5545); RFC 9073 adds PARTICIPANT, VLOCATION and VRESOURCE components (rooms, projectors) that map cleanly to bookable resources (https://www.rfc-editor.org/info/rfc9073); JSCalendar RFC 8984 is the JSON successor (https://www.rfc-editor.org/info/rfc8984). Use `UID` + `SEQUENCE` (Cal stores `iCalUID`, `iCalSequence`) for idempotent two-way updates. CalDAV is RFC 4791 and iMIP is RFC 6047 (both Proposed Standard; https://www.rfc-editor.org/info/rfc4791, https://www.rfc-editor.org/info/rfc6047) [V, RFC Editor metadata].

### 8.4 Rust building blocks [V, crates.io 2026-10-01]

| Need | Crate | Version | Note |
|---|---|---|---|
| iCalendar/JSCalendar/vCard parse + convert, RRULE expansion | `calcard` (Stalwart Labs; crates.io license "Apache-2.0 OR MIT") | 0.3.14 (2026-09-15) | still 0.x; strongest single fit |
| iCalendar builder/parser | `icalendar` | 0.17.14 (2026-09-29) | actively released |
| RRULE | `rrule` | 0.14.0 (2025-04-20) | 17 months since release - watch |
| Timezones | `jiff` | 0.2.37 (2026-09-12) | DST-correct civil/zoned math; 208M downloads; also `chrono-tz` 0.10.4 |
| Cron-style schedules | `croner` 4.0.0, `cron-parser` 0.12.0 | | for SLA sweeps and recurring jobs |
| Inbound/outbound email | `mail-parser` 0.11.9, `lettre` 0.11.23 | | helpdesk email gateway |
| Graphs (deps, critical path) | `petgraph` 0.8.3 | | task DAG and cycle detection |
| Geo | `geo` 0.33.1, `geojson` 1.0.0, `h3o` 0.11.0 | | dispatch zoning |
| Search | `tantivy` 0.26.2 | | ticket/KB search |
| DB/web | `sqlx` 0.9.0, `sea-orm` 2.0.4, `axum` 0.8.9 | | |

The crate ecosystem covers everything except an RFC-complete scheduling (iTIP) engine and a Gantt auto-scheduler; those are build-it-yourself [I].

---

## 9. Fleet, repairs, contracts/service management, customer portal

- **Fleet (Community)** [V, https://github.com/odoo/odoo/tree/20.0/addons/fleet/models]: `fleet.vehicle` (+ model, brand, category, tag, state), `fleet.vehicle.log.contract` (leasing/insurance contracts with cost), `fleet.vehicle.log.services` + `fleet.service.type`, `fleet.vehicle.odometer`, `fleet.vehicle.assignation.log` (driver history). Odoo 20 adds `fleet_maintenance` (vehicle <-> maintenance equipment). Hooks into `hr_fleet` (driver = employee) and `account_fleet` (vehicle costs on bills). Model is small; it is a straightforward asset-with-logs pattern.
- **Repairs (Community)** [V]: `repair.order` with 20.0 states draft / confirmed / done / cancel, parts lines as stock moves, schedule date, parts availability (`available`/`expected`/`late`), `repair_service_line`, lots/serials, linked sale order, stock picking (return receipt), MRP (`mrp_repair`) and POS (`pos_repair`) variants. Helpdesk tickets can create repair orders.
- **Contracts / service management**: Odoo subscriptions (Enterprise) provide recurring plans (billing period, auto-closing, prorated alignment, upsell/renew via portal) - covered in `11-crm-sales-subscriptions.md`; OCA `contract` (AGPL, 19.0 active) is the open equivalent [V repo activity]. For field service, `fieldservice_agreement` + `fieldservice_recurring` link maintenance agreements to recurring orders.
- **Portal** [V]: `project.collaborator` gives external partners `read` / `edit` / `advanced_edit` on a shared project ("project sharing"); helpdesk team visibility `public` lets portal users see only tickets they follow; ratings via `portal_rating`; tasks, tickets, quotes and invoices are all shown under one portal account. Access tokens for anonymous links are the Odoo pattern [G].

---

## 10. Proposed obop service-module decomposition (summary)

Follow Odoo's "thin app + glue module" rule: each app ships independent of sales/accounting, and `*_sale`, `*_timesheet`, `*_helpdesk` glue modules auto-install when both sides exist.

| Module | Core entities | Glue |
|---|---|---|
| `resource` | calendar, attendance, leave, resource (person/material) | used by everything |
| `project` | project, task, stage, state, tag, milestone, update, role, template, recurrence, dependency(+lag, type), collaborator, saved view | `project_sale`, `project_timesheet`, `project_helpdesk` |
| `timesheet` | timesheet entry = analytic line, timer, approval record | `sale_timesheet`, `hr_hourly_cost` |
| `helpdesk` | team, ticket, stage, tag, type, SLA policy, applied SLA, macro, canned reply, rating, KB article/category, email alias, channel | `helpdesk_sale`, `helpdesk_repair`, `helpdesk_fsm` |
| `field_service` | FSM order (task subtype), location/site, equipment, worksheet template + response, skills, territory | `fsm_sale`, `fsm_stock` |
| `planning` | slot/shift, role, template, publish state, swap/self-assign | `planning_project`, `planning_sale` |
| `appointment` | type (collective / round-robin / resource), host, availability override, booking, booking reference, resource/capacity | `appointment_crm`, `appointment_payment` |
| `calendar_sync` | credential, selected calendar, destination calendar, sync token, watch channel | google / microsoft adapters |
| `fleet`, `repair` | vehicle, contract log, service log, odometer; repair order, parts | `repair_sale`, `repair_helpdesk` |
| `portal` | account, access token, share grant | all |

---

## Confidence & gaps

- **High [V]**: Odoo 20 Community source (models, fields, policies), addon presence/absence, licenses and release numbers for OpenProject, Plane, Zammad, Chatwoot, Cal.diy, Vite+ 1.0.0 (2026-09-28, MIT, https://github.com/voidzero-dev/vite-plus) and Rust 1.98.1 (2026-09-03; the stable channel manifest still showed 1.98.1 on 2026-10-01, but the six-week cadence puts 1.99.0 on 2026-10-01 [I], so re-check before pinning), vendor prices as displayed on pages today, Cal.com's closed-source move, Google/Microsoft sync limits.
- **Medium [I]**: Enterprise-only status (inferred from absence in Community repo + editions page, whose check marks did not render); Odoo 20 release timing ("September 2026", page lists no day); Taiga "maintenance mode".
- **Gaps**: Odoo Enterprise models (`helpdesk.ticket`, `planning.slot`, `appointment.type`, FSM) are closed, so the field lists in sections 5-8 come from docs, not source. Sections 5-8 field lists come from the 19.0 docs unless marked Odoo 20; the full Odoo 20 release-notes page is retrievable as raw HTML and its Project, Timesheets, Helpdesk, Planning, Field Service, Appointments and Fleet sections were read on 2026-10-01 (they show that Field Service merged into Planning), but the 20.0 docs for Helpdesk, Timesheets and Appointments were not re-read line by line. Jira SLA mechanics and Jira/Linear data models were not retrievable (Atlassian docs returned only nav); Linear/Jira/Taiga capability cells marked [G] come from memory. No user-sentiment or market-share data were collected. Freshdesk monthly prices and Odoo field-service customer-signature details were not verified. Whether obop will ship a map/routing stack at all is a product decision not yet made.

---

## Implications for obop

**MUST**
1. **Unify time and analytic cost in one table.** Model a timesheet entry as an analytic line (`task_id`, `project_id`, `employee_id`, `so_line_id`, unit of measure hours|days) so project profitability, invoicing and payroll cost share a ledger. This is Odoo's core trick (`account.analytic.line`) and no open PM tool has it.
2. **Implement the service-product policy matrix exactly**: `service_policy` in {prepaid/fixed, delivered-manual, delivered-timesheet, delivered-milestones} x `service_tracking` in {none, task-in-global-project, project+task per order, project only}, with project/task templates on the product. Sales confirmation must create/link project and tasks and set `sale_line_id`; milestone reach must push `quantity_percentage` into delivered qty; per-project employee -> SO line rate mapping is required for consulting firms.
3. **Ship a shared `resource` calendar foundation first** (calendars, attendances, leaves, public holidays, timezones with `jiff`). SLAs, scheduling, planning, timesheet expectations and appointments all depend on it; the SLA and Gantt engines cannot be correct without working-time arithmetic.
4. **Separate stage (user-defined column with fold/closed flag) from state (fixed enum incl. waiting/approved/changes-requested/done/cancelled)** on tasks and tickets, with "waiting" driven automatically by dependencies. Record every stage/state transition in an append-only event table (feeds burndown, cumulative flow, SLA pause intervals, time-in-stage) instead of Odoo's JSON `duration_tracking`.
5. **Build a real SLA engine, not a tag**: policy = criteria filter (team, priority, tags, customer, service) + metric (first response / next response / resolution / reach-stage) + target duration + pause stages + calendar; `applied_sla` rows with status active/hit/missed/active_with_misses (Chatwoot's enum), persisted next-deadline index and a scheduler sweep. Ship it in core, free - Chatwoot, Zendesk, Freshdesk and Odoo all gate it.
6. **Email gateway and alias routing for helpdesk and project** (per-team/project inbound address -> ticket/task, threading by Message-ID/References), using `mail-parser` + `lettre` or equivalent; ticket number in subject as fallback threading key (Zammad pattern).
7. **Two-way calendar sync designed for unreliable push**: store `iCalUID`/`SEQUENCE`, Google watch channels and Graph subscriptions with explicit renewal jobs (Graph events < 7 days, 1 day with resource data), sync-token incremental sync with 410 full-resync path, and a periodic reconciliation sweep. Do not depend on Cal.com or Cal.diy.
8. **Portal as a first-class, token-scoped surface**: customer views of tasks/tickets/timesheets/invoices, project sharing with read / edit / advanced-edit grants, ticket visibility modes (private / internal / public-with-followers), CSAT page. Enforce row-level authorization in Rust at the query layer.

**SHOULD**
9. **Ship Gantt + dependencies with lag and manual/automatic scheduling in the free core** (OpenProject CE does; Odoo gates it). Support relation types relates / duplicates / blocks / precedes-follows (lag in working days, negative allowed) / parent-child; store tasks as a DAG (`petgraph` for validation and critical path).
10. **Helpdesk parity list**: teams with assignment strategies (round-robin, least-open, expertise-tag, availability-aware via leave data), macros, canned replies, merge duplicates, per-stage rating template with 3-point CSAT, knowledge base with publish-to-portal, ticket -> {refund credit note, return, repair, FSM task, CRM lead} actions, SLA reports.
11. **Field service as a task subtype**, not a separate silo (note that Odoo 20 itself moved the other way and models an intervention as a Planning shift with customer, worksheet and sales order, so the task-versus-shift choice is an open design decision): FSM order = task + location + equipment + worksheet + signature + products-used (stock + invoice). Copy the OCA entity set (location, person, equipment, template, team, territory, order type). Worksheets as JSON-schema forms authored in the UI (no Studio-style paid dependency).
12. **Routing/map abstraction** (`RoutingProvider` trait) with OSRM or Valhalla as self-hostable defaults, VROOM as optional VRP solver, MapLibre GL JS for the map UI; never rely on OSM public tiles in production.
13. **Appointments engine** modelled on Cal's EventType / Host / Schedule / Availability-override / Booking / BookingReference, with scheduling types collective + round-robin + resource-with-capacity (Odoo 20 feature), buffers, min notice, cancellation window, optional deposit payment and CRM lead creation. Prefer `calcard` for iCal/JSCalendar and RRULE.
14. **Planning**: shifts with roles, open shifts, publish state, allocated % / hours, recurring generation horizon, and auto-plan by role + availability + leave; integrate with tasks (project) and sales lines (role on service product).
15. **Plan the Gantt/scheduler UI early**: it is the hardest front-end component and should be a reusable canvas/virtualised component built within the Vite+ toolchain (Vite+ 1.0 stable 2026-09-28, MIT; v1.0.0 bundles Vite 8.3.1, Rolldown 1.2.11, Vitest 5.0.1, Oxlint 1.85.0, Oxfmt 0.70.0 and tsdown 0.23.0 and needs Node ^22.18 || ^24.11 || >=26, https://github.com/voidzero-dev/vite-plus/releases/tag/v1.0.0); Plane already runs oxlint/oxfmt, so Oxc tooling is production-proven in this domain.
16. **Expose an MCP server and API parity for services objects** (tasks, tickets, timesheets): OpenProject ships an MCP server (EE Professional+) and Odoo 20 added MCP connectivity to databases; agentic helpdesk is where Zendesk's resolution-priced AI (USD 1.50-2.00 per resolution) is monetising.

**COULD**
17. Sprints/cycles (Linear-style auto-generation, cooldown, rollover, capacity from velocity) as an optional project mode; intake/triage queue (Plane pattern).
18. Project profitability dashboard with expected / to-invoice / invoiced columns on both revenue and cost sides (Odoo pattern), including reinvoiced expenses and vendor bills via the shared analytic account.
19. Fleet and repair modules as simple asset-with-logs plus stock-move-based repair orders; implement after helpdesk, since repair is a helpdesk after-sales action.
20. Contracts/subscriptions service management: reuse the recurring-plan engine from the sales module (see `11-crm-sales-subscriptions.md`) and add FSM agreements for recurring maintenance visits.
21. Position against the market: Odoo Standard EUR 19.90/user (a 12-month introductory price; list EUR 24.90 yearly) vs Zendesk Suite Pro USD 115/agent, Freshdesk Pro USD 55, JSM Premium USD 51.42; a free self-hosted, un-gated services suite is the differentiator, with hosted pricing well below Odoo.
22. Licensing hygiene: read Odoo Community (LGPL-3) and OCA (AGPL-3) code for design only; do not copy OCA modules into a non-AGPL codebase; never use Odoo Enterprise code or assets. Prefer MIT/Apache crates (`calcard`, `icalendar`, `jiff`, `petgraph`) and BSD/MIT geo components.

---

## Verification log

Adversarial fact-check run on 2026-10-01 against live sources (GitHub API and raw files, crates.io API, rust-lang channel manifest, vendor pricing pages, Odoo 19/20 docs and release notes, RFC Editor metadata, Google/Microsoft API docs, OSM policy).

**Confirmed: 75 claim groups** (all 12 author-flagged claims, with the nuances below, plus 63 further groups: release versions/dates/licenses for Plane, Taiga, Zammad, Chatwoot, Cal.diy, OpenProject, OSRM, Valhalla, GraphHopper, VROOM, OR-tools, Photon, MapLibre; all 17 crate versions/dates; Zendesk, Freshdesk, Linear, Jira, JSM, Zammad, Chatwoot, OpenProject, Plane price and tier gating; Odoo 20.0 model fields, enums and module presence; OCA model list; Cal.diy schema; Chatwoot and Zammad models; Odoo 19/20 docs behaviours for SLA, CSAT, Help Center, after-sales, assignment, planning, timesheet suggestions, FSM products/worksheets/Mapbox; Odoo 20 release-note items for MCP, Gantt buffers, billing targets, appointments; RFC numbers/status/update chain; Google push/sync and Graph lifetime facts; OSM tile policy; Linear cycles; OpenProject relations/lag/workflow transitions; Rust 1.98.1; Vite+ 1.0.0 MIT).

**Corrections (was / now / source)**

| Was | Now | Source |
|---|---|---|
| `hr_hourly_cost` is a Community module holding employee hourly cost (sections 2, 4.1) | Module is absent from the 20.0 addons tree (present on 19.0); `hourly_cost` is a field on `hr.employee` in the `hr` module | https://github.com/odoo/odoo/tree/20.0/addons ; https://github.com/odoo/odoo/blob/20.0/addons/hr/models/hr_employee.py |
| Odoo Standard EUR 19.90 / Custom 35.90 presented as the price | These are a 12-month introductory discount for initial users; struck-through list prices are 24.90 / 31.10 (Standard) and 44.90 / 56.90 (Custom); Light User 7.90 | https://www.odoo.com/pricing-plan (page HTML) |
| Enterprise license "OEEL-1 not re-verified" [G] | Verified: Odoo Enterprise Edition License v1.0 (OEEL-1) | https://www.odoo.com/documentation/19.0/legal/licenses.html |
| Field Service treated only as the Enterprise FSM app with projects and tasks | In Odoo 20 the Field Service app is discontinued and merged into Planning; interventions are shifts with customer, sales order, worksheet (property fields, not Studio), sign-in/complete, customer report | https://www.odoo.com/odoo-20-release-notes ; https://www.odoo.com/documentation/20.0/applications/services/planning/field_service.html |
| Project profitability dashboard described as current | Odoo 20 release notes say the project-dashboard profitability report was removed in favour of dedicated budget/margin reports (20.0 docs page still describes it; unresolved) | https://www.odoo.com/odoo-20-release-notes |
| "Odoo 20 mail plugins log Gmail/Outlook mail to tickets" | Plugins log an email in the chatter of an existing contact, task or ticket | https://www.odoo.com/odoo-20-release-notes |
| "one-click rescheduling" | "Reschedule booked appointments in just a few clicks" | https://www.odoo.com/odoo-20-release-notes |
| Cal.diy "commits continue to 2026-09-26" | Default-branch (`main`) last commit 2026-09-20; last push to any branch 2026-09-26 | GitHub API `calcom/cal.diy` |
| Cal.diy schema includes Workflows and routing forms | No Workflow or routing-form models in the Cal.diy schema (Cal.com-only); OutOfOfficeEntry, Team, Membership, BookingSeat do exist | https://github.com/calcom/cal.diy/blob/main/packages/prisma/schema.prisma |
| OCA `field-service` "~37 modules", "branches 16.0-20.0" | 33 `fieldservice_*` modules on 19.0, 37 on 18.0; branches 11.0-20.0; `20.0` is scaffolding with no addons (same for `contract` 20.0) | GitHub API `OCA/field-service`, `OCA/contract` |
| Rust gap: "nothing above ~170 stars" | Larger Rust repos exist but are not comparable: `BloopAI/vibe-kanban` (~28k stars, AI-agent kanban) and `macro-inc/macro` (~4.5k stars, AGPL-3.0 team workspace) | GitHub search API, 2026-10-01 |
| `calcard` "Apache-2.0 badge" | crates.io license is "Apache-2.0 OR MIT" | https://crates.io/crates/calcard |
| Google channel `expiration` "capped by Google" | Expiration is optional, bounded by Google's internal limits; no fixed maximum is documented | https://developers.google.com/workspace/calendar/api/guides/push |
| Graph "calendar latency under 1 minute, max 3" applied to event subscriptions | That row is for the `calendar` resource; `event` latency is listed as "Unknown" | https://learn.microsoft.com/en-us/graph/change-notifications-overview |
| "Zammad gates SLAs to its Professional hosted plan only" | SLAs listed from Professional v2 upward (Plus includes them); Starter v2 does not list them | https://zammad.com/en/pricing |
| Planning swap/self-assign detail [G] | Verified: "Ask to switch" / "I take it", and "Allow Unassignment" / "I am unavailable" (mutually exclusive); Auto Plan constraints added | https://www.odoo.com/documentation/19.0/applications/services/planning.html |
| CalDAV RFC 4791 / iMIP RFC 6047 [G] | Verified Proposed Standards | https://www.rfc-editor.org/info/rfc4791 ; https://www.rfc-editor.org/info/rfc6047 |
| OpenProject "Ruby on Rails [G]" | Verified (Gemfile `rails ~> 8.1.3`) | https://github.com/opf/openproject/blob/dev/Gemfile |
| Zammad channel "phone (Plus)" | Marked (unverified): the pricing page lists phone support hours, not a phone channel | https://zammad.com/en/pricing |
| Jira/JSM prices with no basis | Added note: page JSON-LD gives USD per user/month with no annual/monthly label; marked (unverified) | https://www.atlassian.com/software/jira/pricing |
| Added (missing facts) | OpenProject minimum users (25 / 25 / 100) and Professional EUR 10.95; Zammad hosted prices (EUR 7/16/25 annual); Vite+ 1.0.0 bundled tool versions and Node requirement; Odoo 20 Project and Planning deltas; Rust 1.99.0 due 2026-10-01 per six-week cadence [I] | pages cited inline |

**Unresolved / unverified**

- Jira Standard/Premium and JSM Standard/Premium: whether 7.91 / 14.54 / 20.00 / 51.42 are annual- or monthly-billing rates.
- Freshdesk monthly-billing prices (the page exposes only annual: 19 / 55 / 89).
- Odoo "Enterprise-only" status of helpdesk, planning, appointment, timesheet_grid, web_gantt, sale_subscription remains an inference from absence in the Community tree (editions page check marks did not render); Enterprise models are closed.
- Odoo 20 project profitability: release notes (removed) vs 20.0 docs page (still documented).
- Odoo 20 exact release day (release-notes page says only "September 2026").
- Rust 1.99.0: expected 2026-10-01 by cadence; the stable manifest still showed 1.98.1 when checked, so "latest stable" may change the same day.
- Employees self-assigning arbitrary open shifts in Odoo Planning (only switch/unassign flows confirmed).
- Zammad "phone (Plus)" and the full Zammad channel list; Taiga "maintenance mode" (inference); all section 3.2 cells still tagged [G] for Linear, Jira and Taiga; JSM SLA semantics; Odoo `industry_fsm` naming for the pre-20 Enterprise module.
