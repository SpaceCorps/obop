# Conventions: naming and contract authority

- **Status:** Draft 0.1
- **Date:** 2026-10-01
- **Scope:** The spellings and cross-document contracts that more than one obop document uses: identifier forms, the event catalogue, permission groups, data classes, shared entities and states, hub ports and provider traits, and the rename map that aligns the other documents. It defines no product behaviour.

Reading notes. "Adopted" means this file fixes the spelling or contract for every document. It does not ratify a Proposed record of [DECISIONS.md](DECISIONS.md): tiers, waves, dates, the open/paid list and the D24 renames stay Proposed wherever they appear. "Proposed" marks a contract that rests on such a record, or a design choice this file makes that no owning spec has accepted yet. Q-numbers are the owner questions of DECISIONS section 8. Documents are abbreviated as in the rename map (section 8): `sec-priv` is [spec/security-privacy.md](spec/security-privacy.md), `retail` is [spec/retail-commerce-marketing.md](spec/retail-commerce-marketing.md), and so on. Nothing here is legal advice.

## 1. Purpose, authority and change procedure

**Purpose.** The twenty documents share identifiers that cross files: event types, permission groups, data classes, entities and states, port implementers. This file is the one place where such a name is spelled, so a conforming document can be checked against it.

**Authority.** Order of precedence: DECISIONS.md, then this file, then [ARCHITECTURE.md](ARCHITECTURE.md), then every other document. Where a research note, a spec or ARCHITECTURE.md differs from this file on a name or a shared contract, this file wins. Where this file differs from DECISIONS.md, DECISIONS.md wins and this file is corrected in the same change. This file never settles an owner decision: whatever DECISIONS section 8 lists or marks PROPOSED stays open here and is collected in section 9.

**Division of labour.** This file owns spelling, ownership and the shape of cross-document contracts (who emits, who implements, which class, which state names). The owning spec owns meaning and behaviour (transitions, guards, grants, numbers). A non-owner cites a name and never redefines it.

**Changing a name.**

1. Edit this file first: the registry row and the rename-map row (kind, from, to, documents affected).
2. Then edit every document listed, in the same change or before the next release. A document never carries a name this file does not list.
3. A change that touches a DECISIONS record goes to the owner (section 9); the name stays as listed until the owner answers.
4. A retired spelling may appear only in the rename map and in a one-line "retired" note that links to it.
5. When the generated event catalogue of ARCHITECTURE 4.3 exists, section 3.4 becomes a link; the rules in 3.1 to 3.3 stay.

## 2. Naming rules

### 2.1 Forms by artefact

| Artefact | Form | Examples and notes |
|---|---|---|
| Module id | Lower snake_case; ids of DECISIONS 5.1. A bridge joins two module ids with `_`, the extended flow first | `sales`, `stock_account`, `sale_stock`, `l10n_de`. The module is `sales`; `sale_` is the Odoo-style family prefix of its bridges and entities (`sale_order`), kept |
| Crate, package | Kebab-case, `obop-` prefix; API crate `<module-id>-api` with `_` turned into `-` | `obop-sales`, `sales-api` (not `sale-api`), `l10n-de`. Package id `publisher.name`; `obop.*` is reserved (D32) |
| Model, table, API resource | Singular snake_case, unique in a business. A child entity is the parent's full name plus the part. A generic noun (`team`, `stage`, `plan`) stays with its current owner; other modules prefix (`helpdesk_team`, `ticket_stage`) | `sale_order`, `sale_order_line`, `journal_entry`. Odoo-dotted names (`sale.order`, `account.move`) appear only as Odoo source names in importer mappings and parity columns. Model id, table name and `/api/v1/{model}` segment are the same string |
| Column, field | Snake_case. Foreign key `<entity>_id` with the full entity name (`service_contract_id`, never bare `contract_id`). Money `*_minor` plus `currency_code`; timestamps `*_at`; dates `*_date` or `date`; ranges `*_during`; percent `*_pct`, basis points `*_bp`; booleans `is_*` or `has_*` (existing state flags such as `locked` and `gapless` stay). Columns a module adds to another module's model are `<module_id>__<field>`; business columns `x_<slug>` | `row_version`, `command_id` (one command invocation, D9), `idempotency_key` (a natural dedupe key on a row: job, booking, link). The HTTP header is `Idempotency-Key` |
| Derived progress field | `*_status` for progress along a document flow, `*_state` for a condition | As ARCHITECTURE 5.7 and the owning specs: `delivery_status`, `invoice_status`, `payment_state`, `credit_state`, `approval_state`. No renames |
| State and enum values | Lower snake_case; no spaces, hyphens or capitals; the spelling is `cancelled` (kernel lifecycle, D28). External vocabularies mirrored verbatim stay (iCalendar PARTSTAT, Odoo-mirrored `mailing_trace` statuses, Stripe types inside provider payloads) | `past_due`, `nothing_to_invoice`, `in_progress`. Diagram labels follow the stored values |
| External id | `module.name` (D9, D22) | `sales.order_form` is a view's external id (module `sales`, name `order_form`), not a model |
| Command | Rust type `<Verb><Noun>`; wire id `<model>.<action>`, both derived from one definition | `ConfirmSaleOrder`, `sale_order.confirm`. Not `sales_order.confirm` |
| Event type | PascalCase, section 3.1. Webhook and MCP type: `<aggregate_type>.<past participle>`, generated | `SaleOrderConfirmed`, `sale_order.confirmed`. Opt-in record events `<model>.created`, `.updated`, `.deleted` have no PascalCase twin |
| Extension point | `Before<Verb><Noun>`; a port with a chain of implementers, never an event | `BeforeInvoicePost`, `BeforeTransferValidate` |
| Port, trait | PascalCase. Hub and module ports end in `Port`; provider traits end in the role (`Provider`, `Transport`, `Source`, `Engine`, `Driver`, `Adapter`, `Feed`, `Format`). Call syntax `Port::method` | Section 7. An unnamed "the carrier trait" is wrong: write `CarrierProvider` |
| Permission group | `<module>.<role>`, lower snake_case (section 4). Named action `<module>.<action>`. Standard operations: `read`, `write`, `create`, `delete`, `export`. Cross-app capability rows are bare snake_case | `account.manager`, `stock.validate`, `import_data` |
| API scope | `<model>:<operation>` | `invoice:create_draft`, `party:read` |
| Data class | Lower snake_case, section 5 | `confidential_hr` |
| Account role | Lower snake_case; registry in finance 4.1.1 | `stock_valuation`, `grni`, `cogs` |
| Problem code | Lower snake_case RFC 9457 `type` slug | `idempotency_key_reuse`, `field_not_readable`, `plan_stale` |
| Setting, config | Setting key `<module>.<key>`; database setting `obop.company_ids`; manifest keys as ARCHITECTURE 9.3; environment variables `OBOP_<AREA>_<NAME>` with `_FILE` for secret files (Proposed) | `stock.lock_date`, `obop.company_ids` |
| Decision ids | As the identifier-namespace table of [DECISIONS.md](DECISIONS.md) section 4 | A family that shares letters with another is cited with its document or qualifier ("UI tier F2", "ENGINEERING Q-E2", "finance open question 14", "retail RP11"). Q18 (keep and qualify, or rename) is open; this file keeps and qualifies |

### 2.2 Vocabulary and spelling

- Terms and forbidden words are those of DECISIONS section 4. Additionally: "transfer", not "picking", for the document (`stock_picking_type` and the physical act of picking keep the word); "approvals engine" (D39) for the engine and "approval policy engine" (D18) for its auto, confirm and `approve_by_role` modes; "hub" never alone (hub port, store hub); "timesheet entry", not "timesheet line"; "permission group", never "group" or "role" alone.
- "Class" always carries a qualifier: data class (section 5), retention class, effect class (`read`, `draft`, `commit`, `external`, `money_and_payee`, `destructive`, ai.md 4.3) or `cost_class`. A locked order (`locked` flag), a lock date (`lock_date`) and a stock lock date (a `stock` company setting, not a ledger `lock_date` kind) are three different things.
- Prose is British (licence, organisation, authorised). Identifiers are US English (`authorized`, `fulfillment`, `color`), except `cancelled` and `Cancelled`, which D28 fixes. Modes are `approve_by_role` in code and "approve-by-role" in prose.
- Prose and code: an identifier is always in a code span, spelled as registered. The business thing in prose is lower-case English ("sales order", "quote", "vendor bill", "credit note"); "quote" is a `sale_order` with `order_kind = quote`, and new text prefers it to "quotation". A table or event is never named by its Odoo twin outside parity columns.
- A golden path is cited with its document ("sec-priv GP-2"); the hyphen is optional in local use.
- A spec-local marker family (RP, AZ, AD, C, OQ) is never cited without its document.

### 2.3 Where a name is defined

An event type lives in the emitting module's `*-api` crate; a port in the implementer's `*-api` crate; an entity in the owning spec's entity table; a permission group in the owning spec, and the name is added to the seed list of sec-priv 2 when the spec introduces it. The owner column of each registry below is that document.

### 2.4 Module ids in use

These are the ids that prefix permission groups, crates and events (DECISIONS 5.1 and the specs; names marked Proposed there stay Proposed). Kernel and platform: `base`, `product`, `mail`, `portal`, `analytic`, `automation`, `pricing`, `analytics`, `customize`, `resource`, `calendar`, `discuss`, `documents`, `knowledge`, `sign`, `todo`, `ai`. Finance: `account`, `tax`, `invoicing`, `payment`, `bank`, `edi`, `l10n`, `l10n_de`, `l10n_fr`, `l10n_pl`, `assets`, `deferral`, `budget`, `expenses`, `intercompany`, `consolidation`. Commercial: `crm`, `sales`, `subscriptions`, `rental`, `commissions`. Supply chain: `purchase`, `stock`, `stock_account`, `delivery`, `mrp`, `quality`, `maintenance`, `plm`, `repair`. Retail: `pos`, `loyalty`, `ecommerce`, `website`, `marketing`, `livechat`. People: `hr`, `recruitment`, `attendances`, `payroll`, `appraisals`. Services: `project`, `timesheet`, `helpdesk`, `planning`, `appointments`, `fleet`, `service_contract`. Bridges: `sale_stock`, `purchase_stock`, `sale_project`, `sale_timesheet`, `sale_payment`, `sale_mrp`, `purchase_mrp`, `mrp_account`, `pos_stock`, `pos_account`, `rental_stock`, `subscription_stock`, `expenses_payroll`, `expenses_sales`, `attendances_time_off`, `attendances_payroll`, `recruitment_website`, `hr_resource`. A bridge depends on the `*-api` crates of both modules (R5) and is specified in one spec (7.2).

## 3. Event catalogue

### 3.1 Form of an event name

1. PascalCase `<Aggregate><Fact>`. The aggregate is the registered entity (`TransferDone` is `stock_transfer`) and is never dropped or shortened to a word another module also uses (`ContractActivated`, `ServiceDue` and `AllowanceExhausted` are not acceptable). The fact is a past participle or the adjective of the state entered (`Done`, `Due`, `Overdue`, `Expiring`); a bare noun (`SlaWarning`) is not a fact. "Requested" is allowed for an ask that starts a process (`ReturnRequested`).
2. One name per fact. Where two specs named one fact, the registry below keeps one and section 8 lists the other as retired.
3. Only the owner of the aggregate emits it. A bridge implements handlers and calls ports; it never emits in another module's namespace. A consumer never redefines an event.
4. A command is not an event (`ValidateTransfer` is a command of `stock`), and an extension point is not an event (3.6).
5. Security-log types follow the same form (`ElevationUsed`, `DataRead`) and live in `security_event`; realtime frames (`RecordChanged`, `JobProgressed`) are messages, not events.

### 3.2 Delivery kinds

An event has one name and one or both of two delivery mechanisms. **T**: handlers run in the publisher's transaction and an `Err` aborts the command; use it only for an atomic invariant across modules. **D**: an outbox row with one delivery per subscriber, after commit, at least once, deduplicated by `inbox_processed`. **T+D**: both. **SE**: a security-log event in the append-only `security_event`; **SE+D** is also published for subscribers. A new event is D unless a named T handler needs atomicity. A consumer's handler is listed only by the kind it uses.

### 3.3 Envelope

Every outbox event carries `event_id`, `type`, `schema_ver`, `company_id`, `aggregate_type`, `aggregate_id`, `aggregate_version`, `occurred_at`, `trace_id`, `command_id` of the causing command (Proposed) and `data` (ARCHITECTURE 6.6). Payload columns below are the key data members only. Payloads are thin: ids, versions and the few facts a handler needs; a webhook payload is `id`, `type`, `data.object_id`, `api_version`, with an opt-in snapshot (integrations 4.3). Amounts travel as JSON strings in minor units.

### 3.4 Cross-document events

R23 marks names from R23 D.2 to D.4. Waves follow the emitting module (DECISIONS 5.1) and are Proposed with D25. All other names are Adopted here and Proposed until the generated catalogue replaces this table.

| Event | Kind | Emitter (owner spec) | Named consumers | Payload | Wave |
|---|---|---|---|---|---|
| `RegistryVersionChanged` | D | registry (ARCHITECTURE 5.1) | ux, ai, integrations (OpenAPI cache), platform search, peer nodes | `registry_version` | W0 |
| `PermissionChanged` | D | base authorization (sec-priv 4.2) | ai, knowledge, ux, access cache | `kind` (membership, permission_group, access), ids | W0 |
| `PrincipalInvited`, `PrincipalDeactivated`, `PrincipalSuspended` | D | base principal management (sec-priv 4.1; platform 4.4 shares) | ai (delegations end), people, approvals, notifications, knowledge, sessions, share grants | `principal_id`, `kind` (any principal kind, agents included) | W0 |
| `AgentDelegationRevoked` | D | base (ai 4.2 names it; sec-priv 4.1 adds it) | ai | delegation id | W1 |
| `CompanyCreated` | D | base companies (platform 4.3) | packs (seed templates) | company id | W1 |
| `PartyUpdated` | T+D | base contacts (platform 4.2) | T: contact persons (commercial fields propagate). D: mail, pos snapshots | party id, changed fields | W0 |
| `PartyMerged` | D | base contacts | crm, calendar, documents | winner, losers | W0 |
| `PartyBankAccountChanged` | D | base contacts (platform 4.2; Proposed) | account (approval, payment block), ai anomaly rules | party id, bank account id, change kind | W1 |
| `PartyErased` | D | erasure job (sec-priv 4.6) | search, mail, documents, ai, calendar, chatter, sales, crm, subscriptions, people, services, website, marketing, every app with a contact snapshot | subject party id | W1 |
| `ConsentRecorded`, `ConsentWithdrawn` | D | consent ledger behind `ConsentPort` (sec-priv 4.6) | mail, marketing (suppression on withdrawal) | `consent_record` id, purpose | W1 |
| `ImportRunCommitted`, `ImportRunRolledBack` | D | import engine (platform 4.16) | ux, integrations, party duplicate scan, foreign importers | run id | W1 |
| `ExportRequested` | SE | authorization (sec-priv 4.2) | audit | principal, scope, model | W1 |
| `ExportReady` | D | `export_job` (integrations 4.9) | ux | job id | W1 |
| `MailReceived` | D | mail (platform 4.7) | chatter, documents, automation, ai extraction | mail id, alias, attachment refs | W1 |
| `MailBounced` | D | mail | contacts, marketing | address, kind | W1 |
| `AttachmentCommitted` | D | base attachments (platform 4.1; Proposed), when a blob goes `pending` to `committed` | ai extraction (ai 4.6) | attachment id | W1 |
| `MessagePosted`, `ActivityScheduled` | D | mail (platform 4.5) | NotifyPort, calendar | thread ref | W1 |
| `NotificationCreated` | D | mail notifications (platform 4.6) | ux (Inbox badge) | notification id | W1 |
| `CalendarEventChanged`, `CalendarEventCancelled` | D | calendar (platform 4.8) | appointments | event ref | W2 |
| `ApprovalRequested`, `ApprovalTaskDecided`, `ApprovalDecided` | D | approvals engine (platform 4.12) | NotifyPort, ux; `ApprovalDecided`: ai (releases plan), sign, automation, hr | request id, decision, `plan_digest` | W2 |
| `SignRequestCompleted` | D | sign (platform 4.11) | documents, hr, recruitment, onboarding | request id | W3 |
| `ModuleInstalled`, `ModuleUpgraded`, `ModuleUninstalled` | T | base apps management (platform 4.18) | registry rebuild | module, version | W0 |
| `CredentialAdded`, `CredentialRemoved` | SE+D | base identity (sec-priv 4.1) | mail (user notice) | principal, credential kind | W1 |
| `OAuthConsentGranted`, `OAuthConsentRevoked` | SE | OAuth provider (integrations 4.2) | audit | client, scopes, grant id | W1 |
| `PackActivated` | D | base apps management (platform 4.18; Proposed) | hr, payroll, time off, ux | pack id, version, kind, company | W1 |
| `LanguageActivated` | D | i18n (platform 4.19) | catalogue loader | language | W1 |
| `FxRatesUpdated` | D | base, via `ExchangeRateSource` (finance 4.7; platform 4.3, Proposed) | account (revaluation hint), pricing, analytics, mail | rate type, date, currencies | W1 |
| `ProductChanged`, `VariantCreated` | D | product (sales-crm 4.1) | pos snapshots, website, shop feeds | ids | W1 |
| `PriceRuleChanged` | D | pricing (sales-crm 4.2) | pos snapshot deltas, website tag purge, shop cache | pricelist, rule | W1 |
| `QuoteSent` | D | sales (sales-crm 4.4) | mail, portal, crm timeline, webhooks | order id, link | W1 |
| `QuoteViewed`, `QuoteDeclined`, `QuoteAccepted` | D | portal (sales-crm 4.8) | sales (`quote_status`), crm; `QuoteAccepted`: webhooks | order id, document hash | W1 |
| `SaleOrderConfirmed` (R23) | T+D | sales (sales-crm 4.4) | T: sale_stock (`DemandPort::raise`), crm, sale_project, loyalty, ecommerce. D: subscriptions, rental, commissions, marketing, planning (Proposed), connectors, webhooks, mail, analytics, AI index | order id, lines, untaxed total, currency, opportunity, team, channel | W1 |
| `SaleOrderCancelled` | T+D | sales | sale_stock (cancels open deliveries), sale_project (detaches billing), portal (revokes links) | order id | W1 |
| `LeadCreated`, `LeadConverted`, `LeadsMerged`, `OpportunityStageChanged`, `OpportunityWon`, `OpportunityLost` | D | crm (sales-crm 4.3) | webhooks, timeline, analytics | lead id, stage | W1 |
| `SubscriptionStarted`, `SubscriptionInvoiceDue`, `SubscriptionChanged`, `SubscriptionStatusChanged`, `SubscriptionCancelled`, `DunningStepExecuted`, `MrrMovementRecorded` | D | subscriptions (sales-crm 4.5) | invoicing via `InvoiceableSource` (`SubscriptionInvoiceDue`), service_contract (`SubscriptionChanged`, `SubscriptionStatusChanged`), analytics | subscription id, period, status | W3 |
| `CommissionPeriodClosed` | D | commissions (sales-crm 4.7) | people payroll export, account (draft bill for external agents) | period, statements | W4 |
| `InvoicePosted` (R23) | T+D | invoicing (finance 4.2) | T: sales (`qty_invoiced`), sale_timesheet (locks entries). D: edi, portal, mail, documents (archive), intercompany, assets, deferral, subscriptions, commissions, purchase (vendor bills), pos (attach), webhooks (`invoice.posted`), ai anomaly, analytics | invoice id, number, `direction`, `kind`, party, currency, totals, lines with source refs | W1 |
| `InvoiceReversed` | T+D | invoicing (finance 4.2) | T: sales (quantities). D: commissions (clawback), loyalty (reverse points), stock_account (to confirm, finance 5.2 posts return COGS at `BeforeInvoicePost`), webhooks, ai anomaly | original invoice id, reversing invoice id, `kind` (credit_note, full_reversal), lines with qty, partial or full | W1 |
| `PaymentTransactionDone` | D | payment (finance 4.5) | sales via `sale_payment` (prepayment, Proposed), subscriptions, ecommerce (confirms web order), appointments (deposit), webhooks | transaction id, order ref, amount | W1 |
| `PaymentTransactionFailed` | D | payment | subscriptions (dunning), sales, webhooks | transaction id, reason | W1 |
| `PaymentPosted`, `PaymentVoided` | D | payment | invoicing (`payment_state`), people, webhooks; `PaymentVoided` also sales | payment id, invoice refs | W1 |
| `PaymentReconciled` (R23) | D | payment | invoicing, sales (order `payment_state`), people (claim reimbursed), payroll (run paid), subscriptions, commissions, ecommerce (payment state), ai anomaly, webhooks (`payment.reconciled`) | payment id, statement line | W1 |
| `BankStatementImported` | D | bank (finance 4.5) | suggestion job | statement id | W1 |
| `EntryPosted`, `LockDateChanged` | D | account (finance 4.3) | analytics, automation, audit | entry id or lock kind | W1 |
| `TaxClosingPosted` | D | tax (finance 4.4; not account) | analytics, automation, audit | period, entry id | W1 |
| `TaxRuleChanged` | D | tax (finance 4.4; Proposed; the alternative is retail open question 17) | pos snapshot builder | rule or version id | W3 |
| `EDocumentStatusChanged` | D | edi (finance 4.6) | invoicing (badge), mail, webhooks | e_document id, status | W1 |
| `AssetDepreciationPosted`, `DeferralReleased`, `IntercompanyMirrorCreated` | D | assets, deferral, intercompany (finance 4.9, 4.10, 4.12) | analytics, mail, approvals | schedule line or link id | W3 |
| `TransferDone` (R23) | T+D | stock (supply-chain 4.1.5) | T: sale_stock and purchase_stock (`QtyContributionPort`), stock_account (`PostingPort`), rental (Proposed). D: pos, ecommerce and channel pushers, mrp, webhooks, analytics, AI index, reservation sweep | transfer id, moves, lots, source refs | W1 |
| `TransferConfirmed`, `MoveReserved`, `ReservationLost`, `NegativeStockCreated` | T | stock | quality, chatter, activities | transfer or move id | W1 |
| `StockLevelChanged` | D, coalesced | stock | ecommerce (availability), channel sync, website cache tags, webhooks, analytics, AI index | product, location, quantity | W1 |
| `PurchaseOrderConfirmed` | T+D | purchase (supply-chain 4.2) | T: purchase_stock. D: mail, webhooks | order id | W1 |
| `ShipmentLabelCreated`, `ShipmentTrackingUpdated`, `ShipmentDelivered` | D | delivery (supply-chain 4.9) | customer email, order tracking, channel sync out | shipment id | W3 |
| `ProductionConfirmed`, `WorkOrderStarted` | T | mrp (supply-chain 4.3) | quality; `ProductionConfirmed`: plm | order id | W3 |
| `PosOrderPaid`, `PosOrderRefunded` | T | pos (retail 4.1) | loyalty, pos_stock (pending quantity) | order id, lines | W3 |
| `PosSessionClosed` (R23) | T | pos | pos_stock (`DemandPort`), pos_account (`PostingPort`) | session id, totals by method and tax group | W3 |
| `PosInvoiceRequested` | D | pos | invoicing via `InvoiceableSource` | order id | W3 |
| `PosExceptionRaised`, `ChannelOrderImported`, `ChannelSyncFailed` | D | pos, ecommerce (retail 4.1, 4.3) | notifications; the channel pair also sales | exception type, channel order | W3 |
| `LoyaltyCardChanged`, `LoyaltyOverdraftRecorded` | D | loyalty (retail 4.2) | pos edge snapshot deltas; `LoyaltyOverdraftRecorded`: pos exception | card id, balance | W3 |
| `CartAbandoned`, `CartCheckoutStarted` | D | ecommerce (retail 4.3) | marketing, automation | cart (sale_order) id | W3 |
| `ReturnRequested`, `WithdrawalRequested` | D | ecommerce | stock, invoicing | return_request id | W3 |
| `FormSubmitted` | D | website (retail 4.4) | marketing, crm, helpdesk | form, submission id | W3 |
| `EmployeeCreated`, `EmployeeVersionChanged`, `EmployeeDeparted` | D | hr (people 4.1) | onboarding, recruitment, time off, attendances, payroll, expenses, skills, appraisals | employee id, effective date, changed classes | W3 (core W2) |
| `LeaveApproved`, `LeaveCancelled` | D | hr time off (people 4.2) | resource via `hr_resource` (`ResourceLeavePort`), payroll, attendances, calendar | leave id, dates | W3 |
| `AttendanceClosed`, `OvertimeApproved` | D | attendances (people 4.3) | payroll, time off | attendance id | W3 |
| `ExpenseClaimApproved` | D | expenses (people 4.5) | finance side of expenses (draft bill, unique source ref), payroll via `expenses_payroll` | claim id, lines, employee party | W3 |
| `PayRunConfirmed`, `PayRunPosted` | D | payroll (people 4.10) | expenses (`PayRunConfirmed`); analytics | run id, period | W3 |
| `PayslipConfirmed` | D | payroll | portal and notifications (payslip ready); no account consumer, payroll posts itself through `PostingPort` | payslip id | W3 |
| `ResourceCalendarChanged` | D | resource (services 4.1) | helpdesk (SLA recompute), project (reschedule) | calendar id | W2 |
| `LeaveRecorded`, `LeaveRemoved` | D | resource, written by `hr_resource` | planning, appointments, helpdesk, project, timesheet | resource, interval | W2 |
| `TaskStateChanged`, `MilestoneReached` | T | project (services 4.2) | project (blockers), timesheet (timer); `MilestoneReached`: sale_project | task or milestone id | W2 |
| `TimesheetEntryRecorded`, `TimesheetChanged` | T | timesheet (services 4.3) | sale_timesheet (`QtyContributionPort`) | entry id, sale line | W2 |
| `TimesheetSubmitted`, `TimesheetApproved`, `TimesheetRefused` | D | timesheet | chatter, inbox, webhooks | submission id | W2 |
| `TicketCreated`, `TicketReplied`, `TicketAssigned`, `TicketStateChanged` | T | helpdesk (services 4.4) | SLA maintenance, timers | ticket id | W3 |
| `SlaWarningRaised`, `SlaBreached`, `SlaTargetMet`, `TicketSolved`, `TicketRated`, `TicketMerged` | D | helpdesk | notifications, webhooks, reports | ticket id, applied SLA | W3 |
| `VisitStarted`, `VisitCompleted`, `VisitCancelled` | T+D | planning (services 4.5) | timesheet, stock (van move, `DemandPort`), ticket update, customer report | visit id, `command_id` | W3 |
| `BookingRequested`, `BookingConfirmed`, `BookingRescheduled`, `BookingCancelled`, `BookingNoShow`, `BookingCompleted` | D | appointments (services 4.6) | reminders, crm lead, webhooks, pos tables | booking id | W3 |
| `ExtractionCompleted`, `ExtractionFieldCorrected` | D | ai extraction (ai 4.6) | account (draft via `ExtractionPort`), vendor memory, eval set | job id, draft ref | W3 (ai open question 1 asks whether to pull extraction into W2) |
| `AnomalyRaised` | D | ai (ai 4.8) | inbox | rule, record | W3 (the deterministic controls in the owning apps are W2; ai 4.8 names no emitter for them) |
| `CommandPlanCreated`, `CommandPlanCommitted`, `CommandPlanExpired` | D | command API (ai 4.3) | approvals inbox | plan id | W2 |

### 3.5 Events with no consumer

These events have no named consumer in any document. They are legitimate facts: webhooks, analytics, chatter and the AI index receive durable events without being named, and the owners' Flows sections stay the authority. The list is reviewed when the catalogue is generated.

- Base and platform: `PartyCreated`, `PartyArchived`, `ActivityDone`, `FollowerAdded`, `MailSent`, `TodoItemCompleted`, `TodoItemConverted`, `DocumentFiled`, `DocumentVersionAdded`, `DocumentTrashed`, `DocumentPurged`, `OcrCompleted`, `ArticleUpdated`, `ArticleCommentPosted`, `SignRequestSent`, `SignRequestDeclined`, `SignRequestExpired`, `AutomationRunFailed`, `RuleAutoDisabled`, `ReportGenerated`.
- Commercial: `LoyaltyPointsEarned`, `LoyaltyRewardRedeemed`, `GiftCardIssued`, `PagePublished`, `PageUnpublished`, `CustomDomainActivated`, `MailingScheduled`, `MailingSent`, `EmailComplaintReceived`, `UnsubscribeReceived`, `SuppressionAdded`, `SendingDomainStatusChanged`, `PortalPaymentStarted`, `RentalReserved`, `RentalPickedUp`, `RentalReturned`, `RentalOverdue`.
- Supply chain: `ProductionStarted`, `ProductionDone`, `ProductionCancelled`, `WorkOrderDone`, `UnbuildDone`, `QualityCheckFailed`, `QualityAlertSolved`, `EcoApplied`, `MaintenanceRequestCreated`, `MaintenanceRequestDone`, `EquipmentDowntimeEnded`, `RepairOrderConfirmed`, `RepairOrderDone`, `PurchaseOrderCancelled`, `PurchaseLineQtyChanged`, `ReplenishmentFailed`.
- People and services: `EmployeeReportingLineChanged`, `LeaveRequested`, `LeaveRefused`, `AllocationGranted`, `BalanceExpired`, `AttendanceOpened`, the `Application*` events, `OfferAccepted`, `ExpenseClaimSubmitted`, `ExpenseClaimRefused`, `ExpenseClaimReimbursed`, `EmployeeSkillChanged`, `PlanCompleted`, `AppraisalShared`, `ObjectiveClosed`, `PayRunComputed`, `ShiftPublished`, `OpenShiftClaimed`, `ShiftSwapRequested`, `OdometerRecorded`, `VehicleContractExpiring`, `VehicleServiceDue`, `ServiceContractActivated`, `ServiceContractExpiring`, `ServiceContractAllowanceExhausted`.
- AI and UX: `AskAnswered`, `AiRunCompleted`, `AiToolCalled`, `AiBudgetThresholdReached`, `AiBudgetExhausted`, `AiFeatureToggled`, `AiCredentialRotated`, `SetupStepCompleted`, `PreferenceChanged`, `SavedViewSaved`, `ViewOverridePublished`, `ViewOverrideReset`, `AnomalyDispositioned`, `ForecastRunCompleted`.
- Security log (SE): `LoginFailed`, `SessionRevoked`, `ApiKeyCreated`, `ApiKeyRevoked`, `ElevationUsed`, `AccessDenied`, `AuditChainBroken`, `AuditCheckpointSigned`, `KeyRotated`, `DsarReceived`, `DsarDelivered`, `RetentionExpired`, `DataRead`. Officers and the audit UI read them.

Events emitted and consumed inside one document (for example `EquipmentDowntimeStarted` to mrp, `QualityAlertCreated` to maintenance, `CustomField*`, `WebhookEndpointDisabled`, `ConnectionNeedsReauth`, `SyncRunFailed`) are catalogued by their owner's Flows section and follow 3.1 to 3.3.

### 3.6 Extension points, commands and other non-events

- Extension points: `BeforeInvoicePost` (owner invoicing; implementers stock_account adds COGS lines, edi runs the EN 16931 pre-flight, the tax provider commits) and `BeforeTransferValidate` (owner stock; implementer quality). The owner "runs" an extension point; it is never listed under "Emits" (rename map, section 8).
- Commands named across documents: `PostInvoice`, `CreateInvoice`, `RegisterPayment`, `ImportStatement`, `ReconcileLines`, `PostEntry`, `ReverseEntry`, `CreateQuote`, `SendQuote`, `AcceptQuote`, `ConfirmSaleOrder`, `ValidateTransfer`, `ConfirmTransfer`, `ReserveMove`, `ConfirmPurchaseOrder`, `ConfirmProduction`, `ClosePosSession`. A command is never listed among events (3.1 rule 4).
- Realtime frames: `RecordChanged` (invalidation `(model, ids, row_version)`, ARCHITECTURE 12) and `JobProgressed` (job runner progress, Proposed) travel on the WebSocket and are not outbox events.
- Record events: `sale_order.created` in integrations GP-1 is an opt-in record event, valid as written; no `SaleOrderCreated` exists.

### 3.7 Gaps

| Gap | Resolution path |
|---|---|
| `JobProgressed` is named (platform 4.16, ARCHITECTURE 12) but its shape is not defined | platform 4.16 or ARCHITECTURE 12 defines the frame (Proposed) |
| Named consumers that the emitter's owner lists but the consumer's owner does not: `PayslipConfirmed` (notifications in platform 4.6; portal in sales-crm 4.8), `ShipmentTrackingUpdated` and `ShipmentDelivered` (mail, platform 4.7), `LeaveApproved` (calendar, platform 4.8) and `QuoteSent` (mail, platform 4.7) | A consumption is listed on both sides. Adopted here; the consumers' owners (platform 4.6 to 4.8, sales-crm 4.8) add it |
| livechat hand-off to crm or helpdesk names no mechanism | Open in retail 4.6 and services 4.4; a port call or a `Lead` created through the crm command API |

## 4. Permission-group registry

**Rules.** A group is `<module>.<role>`. The prefix is the owning module id, or the app or engine name where one module hosts several (`contacts` in `base`, `approvals` in `automation`), or a reserved kernel prefix (`base`, `security`, `privacy`, `audit`, `api`). sec-priv 2 owns the kernel seeds and the seed list; a group not yet on the list is added there by its owning spec. The role segment is a noun (`manager`, `operator`). A named action is `<module>.<action>` and is a permission, not a group; it is granted by rows of a group. A field-level permission is a group too (`product.cost_viewer`). Scopes are `all`, `own`, `company`, `domain` (D10); retail's "own session" is `own` and its "branch" scope is a `domain` on `branch_id`. `finance.*` is retired: it is `account.*`.

| Owner spec | Groups |
|---|---|
| sec-priv 2 | `base.user`, `base.portal`, `base.public`, `base.admin`, `base.api_client`, `base.agent`, `security.officer`, `privacy.officer`, `audit.reader`. Capability rows: `customize_views`, `share_views`, `import_data`, `manage_branding` (ux 2); operation `export` (AZ10) |
| finance 2 | `account.read`, `account.billing`, `account.bookkeeper`, `account.user`, `account.adviser`, `account.manager`, `account.payment_approver`. An auditor holds `account.read` plus `audit.reader` |
| people 2 | `hr.self`, `hr.line_manager` (derived), `hr.officer`, `hr.admin`, `hr.hrbp`, `hr.special_reader`, `recruitment.recruiter`, `recruitment.interviewer`, `payroll.preparer`, `payroll.approver`, `attendances.kiosk`. Named actions: `hr.retro_change`, `hr.correct` |
| sales-crm 2 | `crm.user`, `crm.team_lead`, `crm.manager`, `sales.user`, `sales.team_lead`, `sales.manager`, `sales.invoicing`, `product.user`, `product.manager`, `pricing.manager`, `subscriptions.user`, `subscriptions.manager`, `rental.user`, `rental.manager`, `commissions.manager`, `commissions.self`. Field permission `product.cost_viewer`. Named actions `sales.price_override`, `sales.unlock_order` |
| supply-chain 2 | `stock.operator`, `stock.manager`, `stock.valuation_user`, `purchase.user`, `purchase.manager`, `mrp.user`, `mrp.manager`, `mrp.operator`, `quality.user`, `quality.manager`, `maintenance.user`, `maintenance.manager`, `delivery.user`. Named actions `stock.validate`, `stock.unreserve`, `stock.adjust`, `stock.revalue`, `stock.override_negative`, `stock.override_reservation`, `stock.unreserve_others`, `purchase.confirm`, `purchase.approve`, `mrp.confirm`, `mrp.close`, `mrp.unbuild` |
| retail 2 | `pos.cashier`, `pos.supervisor`, `pos.manager`, `pos.admin`, `ecommerce.merchandiser`, `ecommerce.fulfillment`, `ecommerce.manager`, `loyalty.manager`, `website.editor`, `website.publisher`, `website.designer`, `website.admin`, `marketing.user`, `marketing.manager`, `marketing.deliverability_admin`, `livechat.operator` |
| services 2 | `resource.admin`, `project.user`, `project.manager`, `timesheet.user`, `timesheet.approver`, `timesheet.admin`, `timesheet.cost_viewer` (field permission), `helpdesk.agent`, `helpdesk.lead`, `helpdesk.manager`, `planning.user`, `planning.dispatcher`, `planning.technician`, `appointments.host`, `appointments.manager`, `fleet.manager`, `fleet.driver`, `service_contract.manager` |
| platform 2 | `customize.designer`, `automation.designer`, `approvals.designer`, `analytics.designer`, `documents.user`, `documents.manager`, `knowledge.editor`, `knowledge.admin`, `contacts.manager`, `mail.account_admin`, `sign.user`, `sign.admin` |
| ai 2 | `ai.admin`, `ai.user`, `ai.operator`, `ai.auditor`, `ai.hr_feature_owner`, `ai.ap_intake` (service group of the intake agent) |
| integrations 2 | `api.client_admin`, `api.webhook_admin`, `api.connection_admin`, `api.import_operator`, `api.export_owner`, `api.extension_admin` |

**Rulings.**

- **Cashier.** `pos.cashier`; ux's `pos.user` is retired. **HR administrator.** `hr.admin`; ai's `hr.manager` and `hr.user` become `hr.admin` and `hr.officer`; `hr.auditor` is `audit.reader`. The two-key enablement of an HR AI feature is `ai.admin` plus `hr.admin`, with `ai.hr_feature_owner` for the switches.
- **Cost visibility.** One field permission `product.cost_viewer` covers product cost, margin, valuation entries, landed costs, revaluations and manufacturing cost (sales-crm's `catalog.cost_reader` and supply-chain's `cost_viewer` are the same thing; the module is `product`, so `catalog.*` becomes `product.*`). Employee cost is a different audience and stays `timesheet.cost_viewer`. Neither is a data class (section 5).
- **Not groups.** `sales.price_override`, `sales.unlock_order`, `hr.retro_change`, `hr.correct` and all `stock.*`, `purchase.*`, `mrp.*` actions are named actions. `sign_role` (platform 4.11) is a signer slot and `appointment_host` an entity; neither is a group.
- **Overlaps.** `mail.account_admin` manages mailbox connections and `api.connection_admin` the other connections; `api.import_operator` is `import_data` plus staging of foreign-source importers; `export` is a per-model operation and `api.export_owner` governs the full-business export job only. How "business owner only" is held (sec-priv 2 has `base.admin` for both owner and administrator) is a gap for sec-priv and integrations.
- **Agent names.** The example intake agent is `ap-intake` (ai, sec-priv GP-4); "AP Assistant" in platform 5.2 is its display name.
- **Interim HR default.** Whether `hr.officer` reads `confidential_hr` by default is open (section 9); a document that must describe it says so and cites sec-priv open question 17.

## 5. Data-class registry

A data class tags columns. Owner of the registry: sec-priv 4.2 (people 4.11 assigns the HR fields). Classes are not permission groups and not retention classes.

| Class | Meaning | Default readers | Handling |
|---|---|---|---|
| `public_directory` | Directory data: name, work contact, department, title, manager | All internal principals | None; `agent_readable` true |
| `internal` | Non-public business data about people and work: type, schedule, skills | Manager chain, HR | Audit on change; `agent_readable` true |
| `confidential_hr` | Personal employment facts that are not money | Self; `hr.officer` (open, section 9) | Separate tables behind `obop_cls_*`; read audit |
| `special_category` | GDPR Art. 9 data as the pack decides: health, union, diversity, disability, work permit | Self, `hr.special_reader` | Field encryption; purpose on every read; DPIA gate |
| `financial` | Money owed to, paid to or held for a named person or counterparty, and anything from which it can be recomputed | Self, payroll groups, finance groups | Separate tables behind `obop_cls_*`; field encryption; read audit; export approval |
| `secret` (Proposed, sec-priv 4.1) | Credentials and key material: API keys, OAuth and IdP secrets, TOTP seeds, PSP and carrier credentials, webhook secrets, BYOK keys | Nobody; write-only | Envelope-encrypted; absent from exports, logs and errors |

`confidential_hr`, `special_category` and `financial` are the sensitive classes (ARCHITECTURE 7.2): they get `obop_cls_*` roles, never go to a non-local AI route without an impact-assessment acknowledgement (ai 4.5), are not indexed by default (ai 4.7) and are excluded from the browser cache (ux 4.13). `agent_readable` defaults to false except `public_directory` and `internal`. The `obop_cls_*` roles are database roles, not permission groups.

**One class per field type.** A value from which a protected value can be recomputed carries the strictest class of its inputs (D10's inference-channel rule).

| Field type | Class |
|---|---|
| Wage, salary, pay rate, hourly cost, cost rate, timesheet cost snapshot (`cost_rate`, `amount_minor`) | `financial` (timesheet cost also needs `timesheet.cost_viewer`) |
| Payslips, pay run lines, payroll amounts, salary attachments, commission period results, payouts and adjustments | `financial` |
| Bank account numbers and IBANs of parties and employees, bank statement lines, payment batch files, natural persons' tax ids | `financial` |
| Customer payment token references (PSP references) | `financial`; card numbers are never stored |
| Contract terms, national id, birth date, dependants, private address, emergency contact, appraisals, disciplinary records, offboarding reasons, attendance geo, IP and photo, applicant expected salary | `confidential_hr` |
| Sick-leave type and certificates, union, diversity, disability, work permit, biometric attendance | `special_category` |
| PSP API keys, webhook secrets, carrier and transport credentials, IdP secrets, BYOK keys, API keys and OAuth tokens | `secret` (Proposed); until ratified they are vault entries handled as `financial` |
| Product cost, margin, valuation, landed cost, pricelist cost base; party credit limit | No class: field permission (`product.cost_viewer`; finance groups) |
| Project and analytic margins (aggregates over people) | No class; read with `timesheet.cost_viewer`; the minimum group size of people 4.11 for small teams is services' call (Proposed) |
| Signature evidence (IP, user agent), serials and lots linked to a person, ticket requester email | No class; `personal_data` flag |

**Other flags.** `personal_data` (registry capability flag driving DSAR and erasure, D33), `retention_class` (pack-driven, separate taxonomy) and `agent_readable` are per model or class and are not classes. `special-category` as a prose adjective is fine; the identifier is `special_category`. People 2's "public" means `public_directory`; sec-priv AZ5's "confidential and special-category columns" means the three sensitive classes.

## 6. Shared-entity registry and state machines

Owners define states; the rows below fix the names where documents disagreed or both touched the entity.

| Entity | Owner | Canonical names and states | Rulings |
|---|---|---|---|
| `sale_order` (quote, cart, order) | sales-crm 4.4 | Lifecycle `draft`, `confirmed`, `cancelled` plus `locked`; `order_kind` `quote`, `cart`; derived `quote_status` (draft, sent, viewed, accepted, declined, expired), `delivery_status` (pending, started, partial, full), `invoice_status` (`nothing_to_invoice`, `to_invoice`, `upselling`, `invoiced`), `payment_state` (`unpaid`, `partial`, `paid`) | Order `payment_state` summarises its invoices; an invoice `in_process` counts as unpaid for the order. Carts never get a number or `quote_status` (Proposed; sales-crm open question 16, retail open question 6) |
| `cart` | retail 4.3 | Extension row, one per `sale_order` with `order_kind = cart`; states `open`, `checkout`, `pending_payment`, `abandoned`, `expired`, `converted` | Kernel lifecycle stays on the order. The sales channel and the external order id are columns ecommerce adds to `sale_order`, unique on the pair: `ecommerce__sales_channel_id` (full entity name `sales_channel`, 2.1) and `ecommerce__external_order_id` (retail 4.3, ECO-2) |
| `invoice`, `invoice_line` | finance 4.2 | Lifecycle `draft`, `posted`, `cancelled` (drafts only); derived `payment_state` (`unpaid`, `partial`, `in_process`, `paid`), `credit_state` (`none`, `partially_credited`, `fully_credited`) | One model for invoice, bill and credit note, told apart by `direction` and `kind` (finance 4.1.1 lists the values). Credit note: a posted `invoice` that reverses commercial lines; it emits `InvoicePosted` and `InvoiceReversed`. No `vendor_bill` or `account.move` model. `payment_hold` (set by the three-way match through an `invoicing-api` command) is an `invoice` field finance 4.1.1 adds (Proposed) |
| `invoice_line_source` | sales-crm 4.4 | Links invoice line to sale line (qty, kind); unique per `(line, source)` | Written for every posting document; purchase side uses `purchase_line_qty_contribution` kind `billed` |
| `payment`, `payment_batch`, `payment_transaction` | finance 4.5 | `payment`: draft, posted, reconciled, voided. `payment_batch`: draft, confirmed, exported, reconciled. `payment_transaction`: draft, pending, authorized, done, cancelled, error, refunded | `posted` means outstanding entry exists; `reconciled` means bank-matched |
| `expense_claim` | people 4.5 | draft, submitted, approved, refused, cancelled; derived `reimbursement_state` (`not_due`, `to_reimburse`, `in_payroll`, `reimbursed`) | finance 4.11 drops `reimbursed` from the claim states |
| `journal_entry` | finance 4.1.1 | draft, posted, reversed; `entry_kind` values as finance | `lock_date.kind` is `fiscal`, `sale`, `purchase`, `tax`, `hard`; the stock lock date is a `stock` setting |
| `e_document` | finance 4.6 | draft, validated, queued, sent, delivered, acknowledged, rejected, failed, cancelled | |
| `account_role` | finance 4.1.1 | `receivable`, `payable`, `outstanding_receipts`, `outstanding_payments`, `suspense`, `fx_gain_realized`, `fx_loss_realized`, `fx_gain_unrealized`, `fx_loss_unrealized`, `stock_valuation`, `stock_variation`, `grni`, `cogs`, `price_difference`, `inventory_loss`, `production_cost`, `work_center_absorption`, `landed_cost_clearing`, `tax_payable`, `tax_transition`, `deferred_revenue`, `deferred_expense`, `accumulated_depreciation`, `cumulative_translation_adjustment`, `down_payment`, `rental_deposit_liability`, `gift_card_liability`, `purchases` | Role names are snake_case identifiers, not prose; the last four names are Proposed, and an account named in a golden path that is not a role (office expense, payable to employee) stays a chart account. Posting modes `perpetual_at_invoicing` (default), `perpetual_per_transfer`, `periodic`; `periodic` is W2 (supply-chain 3) and finance 5.4 marks that variant W2 |
| `import_run`, `import_mapping`, `import_row` | platform 4.16 | `import_run`: uploaded, mapped, validated, committing, paused, committed, failed, rolled_back, partially_rolled_back; `dry_run_result` (ok, failed) is a field; `posted` is a flag set by accounting importers | `import_batch` is retired; before-images are `audit_change` rows tagged with the run id (Proposed; sec-priv 4.4 adds the cause reference, integrations open question 12 keeps `import_change` as fallback). Platform open question 13 closes on this set |
| `notification` | platform 4.6 | One row per recipient and channel: `delivery_status` (pending, sent, failed) with `failure_reason`; `inbox_state` (unread, read, snoozed, done) with `snoozed_until`; `lane` (mention, activity, alert) | `inbox_item` is retired and the read flag becomes `inbox_state`. Inbox approvals and agent proposals are shown from `approval_task` and `command_plan`, not notification rows. `notification_preference` holds the matrix; `user_preference` references it |
| `command_definition` | ux 4.2 | Palette entry (label, availability, CEL condition, hotkeys) | platform 4.15's `command` is this; "command" alone means a business command |
| `principal` | sec-priv 4.1 | invited, active, suspended, deactivated, for every kind | ai's agent state `retired` is `deactivated` |
| `agent_delegation` | sec-priv 4.1 | active, suspended, expired, revoked | `revoked` added (ai, platform) |
| `api_key`, `credential`, `session` | sec-priv 4.1 | `api_key`: active, expired, revoked. `credential`: one factor (password, totp, passkey, recovery): pending, active, disabled, revoked. `session`: active, `idle_expired`, `absolute_expired`, revoked | ai's agent "credential" is an `api_key` of an agent principal |
| `oauth_client`, `consent_grant` | integrations 4.2 | `oauth_client`: active, suspended, revoked. `consent_grant`: active, revoked, expired | ai's names and states converge here; revoking a client revokes its grants. `consent_grant` (OAuth) is unrelated to `consent_record` (privacy ledger, sec-priv 4.6) |
| `share_grant` | sec-priv 4.1 | active, used, expired, revoked | `used` applies to single-use scope; platform 4.9 adopts the four |
| `consent_record` | sec-priv 4.6 | Append-only, kind `grant` or `withdrawal`, reached through `ConsentPort` | recruitment's `candidate_consent` becomes rows of this ledger with a purpose and expiry (Proposed; people 4.4, which notes that sec-priv 4.6 lists no expiry field) |
| `timesheet_entry` | services 4.3 | draft, submitted, approved, refused; 1:1 extension of `analytic_line`, whose `source_ref` tells timesheet, invoice and bill lines apart (closes services open question 3) | "timesheet line" is retired |
| `resource_work_interval`, `resource_leave` | services 4.1 | Working interval of a calendar; leave kinds `public_holiday`, `time_off`, `closure` | `resource_attendance` is retired; `attendance` means a check-in record (people 4.3) only. `leave_request` (people 4.2, events `Leave*`) and `resource_leave` (events `LeaveRecorded`, `LeaveRemoved`) are different entities |
| Contract entities | each owner | `employment_contract` (people), `service_contract` (services), `vehicle_contract` (services) | No bare `contract` entity, column or event: `employment_contract_id`, `service_contract_id`, `service_equipment_id`. `maintenance_equipment` (own assets, supply-chain 4.6) and `service_equipment` (customer sites, services 4.5) stay separate, and neither is a bare `equipment` |
| `pay_run`, `payslip` | people 4.10 | `pay_run`: draft, computed, confirmed, posted, paid, reversed | `payrun` is retired |
| `subscription` | sales-crm 4.5 | incomplete, trialing, active, past_due, unpaid, paused, cancelled | `canceled` retired |
| `mailing` | retail 4.5 | draft, pending_approval, scheduled, sending, paused, done, cancelled | `canceled` retired |
| `purchase_bill_match` | supply-chain 4.2 | matched, `within_tolerance`, exception | Owns the three-way match (finance 3 keeps only the duplicate-bill fingerprint). A mismatch sets `invoice.payment_hold` and raises an approval item |
| Multi-word state values | supply-chain, people | `quality_alert`: new, confirmed, `action_proposed`, solved, cancelled. `mrp_eco`: draft, `in_progress`, approval, done, cancelled. `appraisal`: draft, `self_review`, `manager_review`, calibration, shared, done | All snake_case |
| Reminders and dunning | finance 4.2, sales-crm 4.5 | Subscription side: `dunning_policy`, `dunning_run`. Invoice reminders: `follow_up_policy` (Proposed name; finance 4.1.1 adds it) | One step-list policy engine serves both (Proposed); neither spec names it or its host module, which stays open |
| `approval_request`, `approval_task` | platform 4.12 | request: pending, approved, executed, rejected, cancelled, expired, superseded; task: waiting, active, approved, rejected, delegated, skipped, expired | `plan_stale` is ai's HTTP 409 for a `command_plan` in `stale`, which is a request in `superseded` |

The `external_id` (module-shipped records), `external_ref` (import identity ledger, integrations 4.8) and `external_mapping` (connector sync, integrations 4.5) are three different tables. `outbox_delivery` states are pending, in_flight, succeeded, exhausted, cancelled (integrations 4.3).

## 7. Hub ports, module ports and provider traits

### 7.1 Hub ports (D27)

D27 (BINDING) names the four hub ports. Implementers beyond the first are Proposed until D27 is amended.

| Port | Implemented by | Called by | Rulings |
|---|---|---|---|
| `DemandPort` | `stock` | sale_stock, pos_stock, mrp, repair, planning (van stock) | `project` is not a caller (services 4.2 does not use it); `planning` is, for van stock. `quality` is not a caller; it uses `StockMovePort` for hold moves |
| `QtyContributionPort` | `sales` (sale lines); `purchase` (purchase lines, Proposed; already in ARCHITECTURE 4.3) | sale_stock and purchase_stock (`('transfer', id)`), sale_timesheet (`('timesheet', id)`), sale_project milestones, service_contract allowance split | Unique on `(line, source)`. `purchase` is a second implementer (Proposed), as GLOSSARY and supply-chain 4.2 list it |
| `PostingPort` (in `account-api`) | `account`, by account role | stock_account, mrp_account, payroll, pos_account, rental (deposit role) | `expenses` is not a caller: an approved claim becomes a draft bill through `invoicing`. Payroll posts itself after a confirmed run |
| `InvoiceableSource` | `sales`, `subscriptions`; Proposed: `purchase` (vendor bills), `pos`, `expenses` via `expenses_sales` | `invoicing` (`CreateInvoice`) | The port is called by invoicing, never the other way round. `timesheet` is not an implementer: billable time reaches invoicing through the sale line. Time-only invoicing is open (section 9.1) |

### 7.2 Module and bridge ports

`StockMovePort` (create, confirm, reserve, cancel) and `StockQueryPort` (availability, forecast): `stock`; callers sale_stock, pos_stock, mrp, repair, quality. `PricingPort::price`: `pricing` (pure, via the Layer 0 crate `obop-calc`, Proposed); callers sales, subscriptions, rental, ecommerce, pos. `ConsentPort` (`base-api`): the consent ledger; callers website forms, pos sign-ups, checkout, marketing, recruitment. `ApprovalPort`: approvals engine; `SignPort`: sign; `NotifyPort`: mail; `SearchPort`: kernel search; `ThreadPort`, `AttachmentPort`, `TemplatePort`, `RenderPort`: platform 4.1; `KnowledgeSearchPort`: knowledge; `EmployeePort` (public lookup, scope test, `create_draft`, `cost_rate`): hr; `SkillPort`: hr; `ResourceLeavePort`: resource (called by `hr_resource`); `ExtractionPort`: ai (optional, called by account and expenses).

Each bridge is specified in exactly one spec and listed in the module table of every spec it touches. `sale_stock`, `purchase_stock`, `stock_account`, `mrp_account`, `sale_mrp`, `purchase_mrp`: supply-chain. `sale_project`, `sale_timesheet`, `hr_resource`: services. `pos_stock`, `pos_account`: retail. `expenses_payroll`, `expenses_sales`, `attendances_time_off`, `attendances_payroll`, `recruitment_website`: people. `sale_payment`, `rental_stock`, `subscription_stock` (all Proposed): sales-crm. supply-chain 4.1.5 lists only its own six bridges and adds `pos_stock`, `rental_stock`, `subscription_stock` and `sale_payment` to its table. `sale_payment` (consumes `PaymentTransactionDone`, confirms the order at `prepayment_percent`) stays a Proposed bridge; whether `payment` instead publishes a hub port is open (section 9.1).

### 7.3 Provider traits and pack hooks

| Trait | Owner spec | Implementers |
|---|---|---|
| `PaymentProvider`, `BankFeed`, `TaxProvider`, `ExchangeRateSource`, `EDocTransport`, `DocFormat`, `AuditFileExporter`, `IntegrityProfile`, `VatIdValidator` | finance 4.4 to 4.14 | Native adapters (X5), pack hooks, connectors (X3 or X4) |
| `CarrierProvider` | supply-chain 4.9 | Aggregator and carrier connectors |
| `PayrollProvider` | people 4.10 | Provider connectors |
| `FiscalAdapter`, `TerminalDriver`, `SmsProvider` | retail 4.1, 4.5 | LocalDevice, CloudSigner, ChainedSoftware; PSP terminals; Twilio first |
| `MailProvider`, `CalendarProvider`, `LookupProvider`, `MessagingProvider` | integrations 4.5 | `CalendarProvider`, `LookupProvider`, `MessagingProvider` are Proposed names; platform 4.2 and 4.8 adopt them |
| `LlmProvider`, `Forecaster` | ai 4.5, 4.8 | Wire-format adapters; augurs baselines |
| `OcrEngine`, `DocEngine`, `SpreadsheetHost`, `RoutingProvider`, `WakeBus`, `BusinessRouter`, `object_store`, `SearchBackend` | platform, services, ARCHITECTURE | As their specs |

Retail's "the `payment` provider trait", "the carrier trait" and "provider trait, BYO Twilio first" are `PaymentProvider`, `CarrierProvider` and `SmsProvider`.

## 8. Rename map

Short names: README, PRODUCT, ARCH (ARCHITECTURE), DECISIONS, ROADMAP, BUSINESS, LOCALIZATION, ENGINEERING, GLOSSARY; specs `platform`, `sec-priv`, `finance`, `sales-crm`, `supply-chain`, `retail`, `people`, `services`, `ux`, `ai`, `integrations`. Where a document only mentions a retired form as history, the fix is to delete or reword the sentence.

| Kind | From | To | Documents |
|---|---|---|---|
| event | `CreditNoteIssued` | `InvoiceReversed` | finance, retail, sales-crm |
| event | `CurrencyRateUpdated` | `FxRatesUpdated` | sales-crm |
| event | `PermissionGroupChanged` | `PermissionChanged` | ARCH, platform, sec-priv |
| event | `ImportBatchCommitted` | `ImportRunCommitted` | ARCH, ux |
| event | `InboundMailRouted`, `MailMessageReceived` | `MailReceived` | ai |
| event | `AttachmentUploaded` | `AttachmentCommitted` (Proposed) | ai |
| event | `OAuthClientAuthorised`, `OAuthClientRevoked` | `OAuthConsentGranted`, `OAuthConsentRevoked` | ai |
| event | `AgentPrincipalCreated`, `AgentSuspended` | `PrincipalInvited`, `PrincipalSuspended` | ai |
| event | `VendorBillDraftCreated` | removed (`ExtractionCompleted` carries the draft ref) | ai |
| event | `SubscriptionCanceled` | `SubscriptionCancelled` | sales-crm |
| event | `data.read` | `DataRead` | people |
| event | `SlaWarning`, `ServiceDue` | `SlaWarningRaised`, `VehicleServiceDue` | services |
| event | `ContractActivated`, `ContractExpiring`, `AllowanceExhausted` | `ServiceContractActivated`, `ServiceContractExpiring`, `ServiceContractAllowanceExhausted` | services |
| event | `CalendarChanged` | `ResourceCalendarChanged` | services |
| event | `InterCompanyMirrorCreated` | `IntercompanyMirrorCreated` | finance |
| event | `TaxClosingPosted` emitted by `account` | emitted by `tax` | finance |
| event | people 4.4 consumes `BookingConfirmed` | removed | people |
| event | `PaymentReconciled` confirms a deposit booking | `PaymentTransactionDone` | services |
| event | `PaymentReconciled` as same-transaction handler; `PaymentTransactionDone` as same-transaction handler | D only | sales-crm |
| event | `PayslipConfirmed` consumed by `account` | not consumed by account; payroll calls `PostingPort` | finance |
| event | sales consumes `PaymentPosted`; edi consumes `CreditNoteIssued` and `InvoiceReversed` | sales consumes `PaymentReconciled` and `PaymentVoided`; edi consumes `InvoicePosted` only | finance |
| event | `PartyErased` not consumed by people and services | consumed | people, services |
| event | `ExportRequested` and `ExportReady` both owned by integrations | `ExportRequested` is a sec-priv security event; `ExportReady` is integrations | platform |
| event | `RegistryVersionChanged` attributed to platform | defined in ARCH 5.1; platform 4.14 only triggers it | ux |
| event | platform 4.4 "emits `PrincipalInvited`, `PrincipalDeactivated`" | base principal management (sec-priv 4.1) emits all three; platform shares the names | platform |
| permission-group | `pos_cashier`, `pos.user` | `pos.cashier` | retail, ux |
| permission-group | `pos_supervisor`, `pos_manager`, `pos_admin` | `pos.supervisor`, `pos.manager`, `pos.admin` | retail |
| permission-group | `shop_merchandiser`, `shop_fulfilment`, `shop_manager` | `ecommerce.merchandiser`, `ecommerce.fulfillment`, `ecommerce.manager` | retail |
| permission-group | `web_editor`, `web_publisher`, `web_designer`, `web_admin` | `website.editor`, `website.publisher`, `website.designer`, `website.admin` | retail |
| permission-group | `mkt_user`, `mkt_manager`, `mkt_deliverability_admin` | `marketing.user`, `marketing.manager`, `marketing.deliverability_admin` | retail |
| permission-group | `livechat_operator`, `loyalty_manager` | `livechat.operator`, `loyalty.manager` | retail |
| permission-group | `warehouse_operator`, `inventory_manager`, `inventory_valuation_user` | `stock.operator`, `stock.manager`, `stock.valuation_user` | supply-chain |
| permission-group | `purchase_user`, `purchase_manager` | `purchase.user`, `purchase.manager` | supply-chain |
| permission-group | `mrp_user`, `mrp_manager`, `mrp_operator` | `mrp.user`, `mrp.manager`, `mrp.operator` | supply-chain |
| permission-group | `quality_user`, `quality_manager`, `maintenance_user`, `maintenance_manager`, `delivery_user` | `quality.user`, `quality.manager`, `maintenance.user`, `maintenance.manager`, `delivery.user` | supply-chain |
| permission-group | `catalog.user`, `catalog.manager` | `product.user`, `product.manager` | sales-crm |
| permission-group | `catalog.cost_reader`, bare `cost_viewer` | `product.cost_viewer` | sales-crm, supply-chain |
| permission-group | `hr.manager`, `hr.user` | `hr.admin`, `hr.officer` | ai, people, sec-priv |
| permission-group | `hr.auditor` | `audit.reader` | people, sec-priv |
| permission-group | `finance.accountant`, `finance.auditor`, `finance.bookkeeper`, `finance.manager`, `finance.payment_approver` | `account.user`, `account.read` plus `audit.reader`, `account.bookkeeper`, `account.manager`, `account.payment_approver` | finance, ai, sec-priv |
| permission-group | `ap_intake` | `ai.ap_intake` | ai |
| permission-group | `sales.price_override`, `sales.unlock_order`, `hr.retro_change`, `hr.correct` listed as groups | named actions, unchanged names | sales-crm, people |
| data-class | `public` (people 2) | `public_directory` | people |
| data-class | "confidential and special-category columns" (AZ5) | the three sensitive classes | sec-priv |
| data-class | hourly cost, cost rate, timesheet cost snapshot in `confidential_hr` | `financial` | people, services, supply-chain |
| data-class | commission period results in `confidential_hr` | `financial` | sales-crm |
| data-class | PSP credentials and tokens in `financial` | credentials `secret` (Proposed), token references `financial` | finance, sales-crm |
| data-class | product cost and margin proposed as class `financial` (stale) | field permission `product.cost_viewer`, no class | supply-chain, sales-crm |
| entity | `import_batch`, `import_change`, states `dry_run_ok`, `dry_run_failed`, `posted` | `import_run`, `audit_change` rows tagged with the run id, `dry_run_result` field, `posted` flag | ARCH, platform, ux, integrations |
| entity | `inbox_item` | `notification` (`inbox_state`) | platform |
| entity | palette `command` | `command_definition` | platform |
| entity | agent `credential` | `api_key` | ai |
| entity | `payrun` | `pay_run` | people |
| entity | `resource_attendance` | `resource_work_interval` | services |
| entity | `contract_scope`, `contract_visit_plan` | `service_contract_scope`, `service_contract_visit_plan` | services |
| entity | `sale.order`, `sales_order.confirm` | `sale_order`, `sale_order.confirm` | sec-priv, ai |
| entity | `vendor_bill`, `vendor_bill:*`, `account.move:draft`, `account.payment:draft` | `invoice`, `invoice:*`, `invoice:create_draft`, `payment:create_draft` | ai, integrations, sec-priv |
| entity | notification matrix inside `user_preference` | `notification_preference` | ux |
| entity | "timesheet line" | "timesheet entry" | services |
| field | `contract_id` on `employee_version` | `employment_contract_id` | people |
| field | `contract_id` on `ticket`, `service_equipment`, `contract_scope`, `contract_visit_plan` | `service_contract_id` | services |
| field | `equipment_id` | `service_equipment_id` | services |
| field | "salary field" (GP-2) | `wage` | ai |
| state | subscription and mailing `canceled` | `cancelled` | sales-crm, retail |
| state | principal `retired`; `oauth_client` registered, removed | `deactivated`; active, revoked | ai, integrations |
| state | `agent_delegation` lists lacking `suspended` or `revoked` | active, suspended, expired, revoked | platform, ai, sec-priv |
| state | `share_grant` without `used` | active, used, expired, revoked | platform |
| state | `consent_grant` without `expired` | active, revoked, expired | ai |
| state | claim state `reimbursed` | derived `reimbursement_state` | finance |
| state | `nothing to invoice`, `to invoice` | `nothing_to_invoice`, `to_invoice` | sales-crm |
| state | `partially credited`, `fully credited` | `partially_credited`, `fully_credited` | finance |
| state | `action proposed`, `in progress`, `within tolerance` | `action_proposed`, `in_progress`, `within_tolerance` | supply-chain |
| state | `self review`, `manager review`; `not due`, `to reimburse`, `in payroll`; capitalised pay run states | snake_case, lower case | people |
| state | `idle-expired`, `absolute-expired`; capitalised erasure states | `idle_expired`, `absolute_expired`; lower case | sec-priv |
| port | `sale-api` | `sales-api` | sales-crm, supply-chain |
| port | `DemandPort.raise` | `DemandPort::raise` | sales-crm |
| port | "the carrier trait", "payment provider trait", "provider trait, BYO Twilio" | `CarrierProvider`, `PaymentProvider`, `SmsProvider` | retail |
| port | `InvoiceableSource` row of finance 4.15; "timesheet is an implementer" | implemented by `sales`, `subscriptions` (and Proposed others), called by `invoicing`; not timesheet | finance, services, GLOSSARY |
| port | `QtyContributionPort` "sales only" | `sales` and `purchase` (Proposed) | GLOSSARY, supply-chain |
| port | `DemandPort` callers include `project` | `planning` | ARCH |
| port | `expenses` as `PostingPort` caller | not a caller | finance |
| port | `candidate_consent` without `ConsentPort` | ledger rows through `ConsentPort` (Proposed) | people |
| other | "approval engine" | "approvals engine" | DECISIONS, ROADMAP, platform, sec-priv, services, supply-chain, ai |
| other | "one picking" | "one transfer" | supply-chain |
| other | prose role names (stock valuation, GRNI, COGS, deferred revenue) | snake_case account roles | finance |
| other | "Consumed from Platform: website" | "Consumed from Retail" | people |
| other | ROADMAP ids `S7`, `P1`, `P1 to P8` | `SR7`, `PR1`, `PR1 to PR8` | BUSINESS, PRODUCT, retail |
| other | effect values without `money_and_payee`; operation metadata without `cost_class` | add both (Proposed) | ai, ARCH |
| other | extension points listed under "Emits" (`BeforeInvoicePost`, `BeforeTransferValidate`) | listed under "Runs" (not an event) | finance, supply-chain, sales-crm |
| other | agent name `ap-agent` | `ap-intake` | sec-priv |
| other | finance capability "Duplicate-bill fingerprint, three-way match" as finance | duplicate-bill fingerprint stays in finance; three-way match belongs to `purchase` | finance |
| other | `periodic` posting mode shown as a W1 variant (finance 5.4) | W2 | finance |

## 9. Open items that affect names

### 9.1 Owner decisions

These stay open. Documents keep "Proposed" wording and cite the question; this file does not pick a side.

| Item | Why it matters for names | Reference |
|---|---|---|
| Tier and wave labels T0 to T3, W0 to W4; `customize` and `resource` as T0 modules; the hr employee core in W2 | Module ids `customize` and `resource`, and every wave value in sections 3 and 4 | D25, D26, Q8 |
| Renames `TenantTx` to `BusinessTx`, `TenantRouter` to `BusinessRouter`, `tenant_release_state` to `business_release_state`; prefixes X, L, W | The names used throughout this file are the Proposed ones | D24, Q10 |
| Identifier families that share letters: keep and qualify, or rename | Section 2.1 follows "keep and qualify" | Q18 |
| POS order capture at 1.0 or in W3 | Wave of every `pos` event, the `pos-api` contract and the `pos.*` groups | D13, D25, Q11 |
| Whether `payment` publishes a hub port instead of the `sale_payment` bridge | A new hub port amends D27 | D27; sales-crm open question 6 |
| Time-only invoicing, that is whether `timesheet` implements `InvoiceableSource` | The implementer list in 7.1 | ARCHITECTURE 16.3 item 7; services open question 14; finance open question 18; Q8 |
| Whether `hr.officer` reads `confidential_hr` by default, or each class needs an explicit grant | Default readers in section 5 | sec-priv open question 17; people open question 14 |
| The working name "obop" | The `obop-` crate prefix, `obop.toml`, the reserved `obop.*` package namespace, `@obop/*` and `OBOP_` variables all change with it | D3, Q2 |
| Pack wave and country order (`l10n_fr`, `l10n_pl`, AT and CH) | Pack module ids and their wave column | D4, D25, LOCALIZATION open questions |

### 9.2 Proposed rulings awaiting the owning spec

These are not owner decisions. The named spec accepts, amends or rejects each when next edited; until then they are Proposed.

| Ruling | Spec that decides |
|---|---|
| `secret` as a sixth data class; `api.export_owner` and how "business owner only" is held | sec-priv 4.1, 2; integrations 2 |
| Pay data (wage, cost rate, commission) in `financial`; cost and margin as field permission `product.cost_viewer` and no class | people 4.11, services 2, sales-crm 4.7, supply-chain 4.1.6 (closes sales-crm open question 17 and supply-chain OQ-17) |
| `audit_change` carries the import run as cause; `import_change` is the fallback | sec-priv 4.4; integrations open question 12 |
| Recruitment consent as ledger rows through `ConsentPort`, replacing `candidate_consent` | people 4.4 |
| `purchase`, `pos` and `expenses` as `InvoiceableSource` implementers; `purchase` as `QtyContributionPort` implementer | supply-chain 4.2, retail 4.1, people 1.2; D27 note |
| Emitters of `PackActivated`, `PartyBankAccountChanged`, `AttachmentCommitted`, `TaxRuleChanged` and `FxRatesUpdated`: adopted in the owning specs as Proposed, so they stay Proposed. `JobProgressed` frame shape | platform 4.1, 4.2, 4.3, 4.18 and finance 4.4 (emitters); platform 4.16 or ARCHITECTURE 12 (frame) |
| `obop-calc` as the Layer 0 crate behind `PricingPort` and the tax engine | ARCHITECTURE 4.1; sales-crm 4.2 |
| Whether `Idempotency-Key` is required on plain CRUD or only accepted (integrations 4.1 "carries", ARCHITECTURE 8.2 "accept") | integrations open question 11 |
| Device identity: kiosk and POS devices are API-client principals plus a device record (`pos_device`), with no new principal kind (D28) | people 2, retail 4.1 |

Facts that are not names (dates, version pins, counsel timing, the 6% grant coverage, Astro wording, lockfile lists, the exit drill) are outside this file: they live in [DECISIONS.md](DECISIONS.md) or in the owning document.

## 10. Sources

All paths are relative to this file.

| Tag | File |
|---|---|
| Register | [DECISIONS.md](DECISIONS.md) sections 4 (vocabulary, identifier namespaces), 5 (tiers, waves), 8 (owner questions) and D9, D10, D18, D24, D27, D28, D33, D39 |
| Architecture | [ARCHITECTURE.md](ARCHITECTURE.md) sections 4.3 (hub ports, event names), 5.2, 5.7, 6.6, 7.2, 8, 9.3, 12 |
| Specs | [spec/platform.md](spec/platform.md), [spec/security-privacy.md](spec/security-privacy.md), [spec/finance.md](spec/finance.md), [spec/sales-crm.md](spec/sales-crm.md), [spec/supply-chain.md](spec/supply-chain.md), [spec/retail-commerce-marketing.md](spec/retail-commerce-marketing.md), [spec/people.md](spec/people.md), [spec/services.md](spec/services.md), [spec/ux.md](spec/ux.md), [spec/ai.md](spec/ai.md), [spec/integrations.md](spec/integrations.md) |
| Other | [GLOSSARY.md](GLOSSARY.md), [ROADMAP.md](ROADMAP.md), [BUSINESS.md](BUSINESS.md), [PRODUCT.md](PRODUCT.md), [LOCALIZATION.md](LOCALIZATION.md), [ENGINEERING.md](ENGINEERING.md), [../README.md](../README.md) |
| Research | [research/23-kernel-model-architecture-decisions.md](research/23-kernel-model-architecture-decisions.md) D.2 to D.4 (R23 names), [research/11-crm-sales-subscriptions.md](research/11-crm-sales-subscriptions.md) MUST 6 (the retired `CreditNoteIssued`) |
