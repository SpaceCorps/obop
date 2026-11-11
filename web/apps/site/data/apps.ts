/**
 * Hand-written copy per app. Everything structural (models, fields, workflows, groups, approvals) is
 * read from packages/*.toml and data/builtin/*.toml at build time; only the human "what it is for"
 * text lives here. English only; the one-line taglines are translated in i18n/<locale>.ts.
 *
 * Where a caveat goes (the rules for `limit`, `limits` and `technical`):
 * - The plain text on top (`purpose`, a card's `text`) says what the app does and what a person needs
 *   to know to use it safely: a step that does not do what its name suggests ("By itself, this does not
 *   notify anyone"), an edit that is still possible after a step, a legal or privacy limit. It is true on
 *   its own, is not empty and is at least three words: a reader who never opens the fold is not misled
 *   by what is left. A caveat of that kind stays on top.
 * - A caveat that only a developer or an operator needs (how a check is made, what is not tested against
 *   a live service, a field nothing reads, a template that nothing links) goes under "For developers":
 *   on a card as `limit`, for the app as an entry of `limits`. A limit is one sentence that ends with
 *   a full stop and has no `<`, `>`, `*` or `|` and no `-` or `#` at its start (it is written into a
 *   Markdown list). It is English, so a translated page marks it as English. It is shown only in the
 *   fold, under "What the package does and does not do", never next to the card.
 * - A card may carry a `limit` only if it has a `cards` entry here, since the card's `text` is the plain
 *   top sentence. The limit is not a copy of the top text: the sentence is moved, so it is taken out of
 *   `text` (or `purpose`).
 * - A sentence the fold already has (`technical`, `packageText`, a `fieldText`) is deleted from the top,
 *   not copied into a `limit`.
 * `scripts/lib/app-view.test.ts` checks, for every card or app that carries a `limit` or `limits`, that
 * it is one sentence without markup, that the text left on top has at least three words and does not
 * contain it, and that it is in the part for developers of the Markdown copy and not above it (the
 * browser test reads the page); it also lists the caveats that exist, so a new one is a deliberate
 * change, and pins the wording of some. Whether a caveat is only for a developer, and whether what is
 * left on top is true on its own, is a judgement that no test makes.
 */
export type Category = "foundation" | "customers" | "finance" | "operations" | "people" | "content";

export interface AppCopy {
  /** Module id as in the manifest (`field_service`). */
  id: string;
  category: Category;
  /** Short name used on tiles and headings. */
  name: string;
  tagline: string;
  /**
   * What it is for, in plain words for a person who does not build software: the first sentence is
   * set large, so it is one short, plain sentence; the rest says what the app will and will not do.
   */
  purpose: string;
  /**
   * The technical remainder of what the app does (an API, a hash, a data class), in a paragraph that
   * sits under "For developers" and not in the plain text above.
   */
  technical?: string;
  /**
   * Plain names for the few models whose technical name reads oddly on a card, by model name
   * (`accounting_move` is "Journal entry"). The others are the model name in words.
   */
  modelNames?: Record<string, string>;
  /**
   * Plain words for a card whose manifest text a visitor cannot read (a group name, a hash, a claim the
   * module does not keep), by `model.action` for an action card and by model name for a model card. The
   * manifest stays the source of every fact: this says the same thing in plain words, or says less. It is
   * English; a translated page shows such a card in English, marked as such, because the product's
   * translation says the old words.
   */
  cards?: Record<string, CardCopy>;
  /**
   * The package text under "For developers" (what the package does and does not do), where the
   * manifest's own text says more than the module does. The site cannot change the module's words, so
   * it shows these instead; the sentence that was left out or reworded is named in a comment next to
   * each, and the module's text is the thing to correct at its source.
   */
  packageText?: string;
  /** The same for the description of one model in the technical part, by model name. */
  modelText?: Record<string, string>;
  /** The same for the description of one field in the technical tables, by `model.field`. */
  fieldText?: Record<string, string>;
  /**
   * Models that have no `delete` command, by model name. The technical part lists `model.delete` among
   * the steps an AI agent can only propose, for every model, which is right where the module
   * generates the command; a module that makes a model read-only, or refuses an AI agent's delete
   * even once a person approves it (Sign's links), lists it here so the page does not name a step
   * that can never complete.
   */
  noDelete?: string[];
  /**
   * Caveats of the app as a whole that only a developer or an operator needs, one sentence each. They
   * are shown under "For developers", in the package part, after `technical`, and not in the plain
   * text above (see the rules at the top of this file).
   */
  limits?: string[];
}

export interface CardCopy {
  /** The card's heading, when the words of its action name read oddly. */
  title?: string;
  /** The plain sentence or sentences on the card; true on its own (see the rules at the top of this file). */
  text: string;
  /**
   * A caveat of this card that only a developer or an operator needs, one sentence. It is shown under
   * "For developers", headed by the card's name, and not on the card.
   */
  limit?: string;
}

export const BUILTIN_IDS = ["base", "product", "crm", "sales", "invoicing", "connectors"] as const;

export const APP_COPY: AppCopy[] = [
  {
    id: "crm",
    category: "customers",
    name: "CRM",
    tagline: "Follow your sales leads",
    purpose:
      "Keep track of possible sales, from the first contact until you win or lose them. Each lead has a title, a contact name and email, the revenue you expect and a stage. Moving a lead through the steps of a sale is a single action that your team and its AI agents can both take.",
  },
  {
    id: "sales",
    category: "customers",
    name: "Sales",
    tagline: "Quotations and orders",
    purpose:
      "Write quotes for customers and turn them into confirmed orders. Each order can name a contact and has lines, a total and a currency. Confirming an order prepares a draft invoice with the same lines, for Invoicing to post. The person who confirms also needs access to Invoicing.",
    // What confirm and cancel do (crates/modules/sales/src/lib.rs:56-103): confirm creates a draft invoice and its lines,
    // cancel has no effect (:91-95), reset_to_draft then confirm creates a second invoice, and the order and its
    // lines stay writable by sales.user (:18, :109), so an edit after confirming does not reach the invoice.
    cards: {
      "sale_order.confirm": {
        text: "Confirm a draft order and prepare its draft invoice. A confirmed order can still be edited; the invoice does not follow. If the order is put back to draft and confirmed again, another draft invoice is prepared.",
      },
      "sale_order.cancel": {
        text: "Cancel an order. Its draft invoice, if it has one, is not cancelled.",
      },
    },
  },
  {
    id: "invoicing",
    category: "finance",
    name: "Invoicing",
    tagline: "Bill your customers",
    purpose:
      "Prepare invoices for your customers as drafts, then post each one: posting gives it its own number, in a yearly series with no gaps. Posting needs a person with the right permission: an AI agent can prepare the draft and propose posting it, but cannot post it. Posting does not lock an invoice: its lines and amounts can still be changed afterwards.",
    cards: {
      invoice: {
        text: "A customer invoice: its lines, a total that adds them up, and a number once it is posted.",
      },
      "invoice.post": {
        text: "Post a draft invoice and give it its next number in the yearly series. Refused if the invoice has no amount.",
      },
    },
    // The module says "posted invoices are corrected only by credit notes" (crates/modules/invoicing/src/lib.rs:17):
    // there is no credit note in the module, and nothing locks a posted invoice's fields. What holds is
    // that no action leaves "posted" (lib.rs:72-85 is the whole list of actions: post, cancel). The
    // module's number field says "legal number" (lib.rs:64) and its post action "assign its legal
    // number" (lib.rs:75): the module gives a gapless number of the year (lib.rs:69-70,
    // `.sequence(.., true, Reset::Year).assign_on("post")`) and makes no claim about the law, so the
    // site says that. A post is refused for an invoice with no amount (lib.rs:79-82). Posting locks nothing:
    // invoice and invoice_line have `ext: Default::default()` (lib.rs:87, :149), so no immutable_when (the guard
    // exists in crates/kernel/obop-registry/src/ext.rs:112-128, and no module or package uses it), and
    // invoicing.user writes the lines (:93).
    packageText:
      "Customer invoices with lines: drafts, posting with a gapless yearly number, cancellation. Posting does not lock an invoice. There are no credit notes, and payments are not tracked.",
    modelText: {
      invoice: "A customer invoice. A posted invoice cannot go back to draft.",
    },
    fieldText: {
      "invoice.number":
        "Gapless yearly number such as INV-2026-00042, assigned when the invoice is posted",
    },
  },
  {
    id: "product",
    category: "foundation",
    name: "Products",
    tagline: "Your products and prices",
    purpose:
      "The list of everything you sell, with its name, product code, price and cost. Cost is shown only to people who may see costs, so a salesperson can quote a price without seeing your margins.",
  },
  {
    id: "base",
    category: "foundation",
    name: "Contacts",
    tagline: "People and companies",
    purpose:
      "One record for every person and company you deal with, used by Sales, Invoicing, Purchase and other apps. Names and email addresses are personal data, so AI agents see them only as placeholders, and the tax number is kept from them completely. A separate erase step, for the privacy officer role, replaces a contact's details with [erased].",
    technical:
      "The erase action anonymises a contact in place and is gated by approval. Names and emails are masked for agents; the tax identifier is withheld.",
    modelNames: {
      party: "Contact",
    },
    cards: {
      "party.erase": {
        text: "Replace a contact's name, email, tax number and notes with [erased], and remove the photo. The Connectors app also deletes the code names and links it kept for this contact. Names, emails and addresses typed into other records (such as leads, orders and tickets) are not changed. Backups made before the erase still hold the old details until they are deleted, a restored backup brings them back, and anything already sent to another system through a connection is not recalled. A placeholder that an AI agent already holds keeps standing for the old name for up to a day after the agent last used obop. For an agent that reaches obop through the obop mcp command, the placeholder keeps standing until that command stops.",
        limit:
          "The erase does not touch the list of placeholders an AI agent holds; that list is forgotten 24 hours after its last change for an agent that uses the REST API, 2 hours after the last request for an MCP session over HTTP, and when the command stops for an agent that reaches obop through obop mcp.",
      },
    },
    // The placeholder list of an AI session is not touched by the erase: Vault::erase_record
    // (crates/kernel/obop-masking/src/lib.rs:151) has no caller outside tests, the REST list is kept
    // 24 hours after its last change (crates/kernel/obop-http/src/shared.rs:20, :110), an MCP session over
    // HTTP 2 hours after its last request (crates/kernel/obop-http/src/mcp.rs:49, :78), the `obop mcp` command
    // (crates/obop/src/main.rs:1766-1769) keeps one Session, its vault in memory, until its input closes and
    // the process ends (crates/kernel/obop-mcp/src/lib.rs:51, :498-512), and restore
    // reads it (crates/kernel/obop-commands/src/lib.rs:1306-1311). Backups keep the old details
    // (docs/DEPLOY.md:597), a restore does not replay an erasure (docs/RUNBOOK-RECOVERY.md:114) and a
    // delivery already made is not recalled (docs/CONNECTORS.md:168).
    // The erase sets name, email, tax_id and notes to "[erased]" and clears the photo
    // (crates/modules/base/src/lib.rs:97-103). The Connectors module, which is built in, listens for
    // party.erase (crates/modules/connectors/src/lib.rs:28, crates/kernel/obop-connectors/src/runtime.rs:1391-1416)
    // and deletes the pseudonym, sealed-value and external-id entries of "party:<id>"
    // (obop-connectors/src/egress.rs:643-693). Nothing else reacts to the command (a search for
    // "party.erase" in crates/ finds only those two, the error tests and the CLI test), and a lead keeps
    // its contact name and email as plain text with no reference to a contact (crates/modules/crm/src/lib.rs:26-39).
  },
  {
    id: "connectors",
    category: "foundation",
    name: "Connectors",
    tagline: "Connect your other tools",
    purpose:
      "Connect your other tools to obop, with privacy controls built in. These are technical controls that support GDPR compliance, not legal advice or a certification. The catalogue says for every service how much of it is built. No record goes out or comes in through a connection until the reason for sharing is written down and the details that may be shared are listed. Google Drive is the exception: once an administrator has signed in to Google, people with access can browse and import files, and, if allowed, add files, without a recorded reason. Fields marked as personal data go out under a code name (a pseudonym) unless you decide otherwise. A field that is not marked as personal data, such as a description, goes out as written if you list it for sharing. Sensitive fields stay back by default; a person can release a sensitive kind for a connection only by writing down an impact assessment, and secrets never leave. Every record that goes out or comes in through a connection is logged without its content; Google Drive imports and uploads are logged in the security log instead, and the shop's card payments through Stripe are not in this log. When the owner of the connection, or an administrator, withdraws the reason for sharing, the connection ends and obop deletes its copy of the sign-in details. A list of what each connection processes is also kept, and you can export it as a starting point for your own record of processing (GDPR Article 30).",
    technical:
      "Fields marked as personal data leave as keyed pseudonyms by default, the sensitive data classes stay withheld by default (a person can release one for a connection with a recorded impact assessment; secrets never leave), every flow through a connection is recorded without its payload, and withdrawing consent disconnects the service and deletes obop's copy of its credentials. The processing register is a generated read model over the connections, providers and consents, exportable as CSV; the complete Article 30 record stays with the operator.",
    modelNames: {
      connector_provider: "Service",
      provider_policy: "Service settings",
      egress_policy: "Data sharing settings",
      sync_run: "Run",
      data_flow_log: "Transfer log",
    },
    cards: {
      "connection.connect": {
        text: "Start sharing data. Needs the reason for sharing written down and complete settings.",
      },
      "connection.test": {
        text: "Check the settings and the address. For some services that receive records from obop, it also sends one empty test event (no records, no personal data) to that address.",
      },
      "connection.revoke": {
        text: "End the connection for good. obop stops using it and deletes its own copy of the sign-in details and code-name lists. The key or authorisation you gave obop stays valid at the other service until you remove it there.",
      },
      "consent.withdraw": {
        text: "Withdraw the reason for sharing (the connection's owner or an administrator): the connection ends and obop deletes its copy of the sign-in details.",
      },
      provider_policy: {
        text: "What an administrator decided about one service: whether it is on, for whom, why data may be shared, and the agreement.",
      },
      connection: {
        text: "A link to one other system, with its sign-in details, the list of details that may be shared, its rules and its state.",
      },
      consent: {
        text: "The reason for sharing recorded for a connection: who, why, which kinds of data, and whether it was withdrawn.",
      },
      data_flow_log: {
        text: "A record of every transfer through a connection, written by obop and not edited by people: when, what, how many, which details, how they were treated, and a fingerprint of the content. Never the content itself. Entries older than the retention period are removed by the purge.",
      },
      processing_register: {
        text: "A generated list of what each connection processes, as a starting point for your own record of processing under GDPR; exportable as CSV.",
      },
      egress_policy: {
        text: "Switches for the whole instance: the master switch, EU-only services (off until an administrator turns it on), whether an agreement is required, and the allowed hosts.",
      },
      privacy_preview: {
        text: "What a sample record looks like after the privacy rules, so you can check it before anything is sent.",
      },
    },
    // What the page says about the connection steps, and the code that decides it:
    // - test sends one empty event ("obop.test", no records) to a webhook service, except for the services
    //   that take form or DocuSeal bodies (crates/kernel/obop-connectors/src/runtime.rs:1279-1313; the manifest's
    //   "without sending personal data" is true, "check the address" hid the request).
    // - revoke blanks credentials and oauth_tokens in obop's own store, drops the queue and forgets the code
    //   names (runtime.rs:1351-1360); no code asks the other service to cancel the key or authorisation
    //   (nothing in oauth.rs, net.rs or catalog.rs revokes at a provider).
    // - withdraw is for the connection's owner or an administrator (own_record, crates/modules/connectors/src/guards.rs:86-103,
    //   :668); the consent carries the connection owner's id as its owner_user (handlers.rs:323-381), a data subject has no step.
    // - the personal-data code name, the sensitive default and the release by impact assessment are
    //   crates/kernel/obop-connectors/src/policy.rs:3-6, :66-73, :86, :221-240; only a field marked as personal
    //   data is pseudonymised, any other listed field is allowed as written.
    // - EU-only is off and the agreement is required by default (crates/kernel/obop-connectors/src/model.rs:187-196).
    fieldText: {
      "egress_policy.eu_only":
        "When on, personal data only goes to providers with an EU or adequacy basis. Off until an administrator turns it on.",
      "egress_policy.require_dpa":
        "When on, personal data, even pseudonymised, only goes to providers whose agreement is recorded. On by default.",
    },
    // The module calls the log "Append-only" (crates/modules/connectors/src/meta.rs:233, snapshot in
    // data/builtin/connectors.toml), but the purge deletes its rows once they are older than the
    // retention of their connection (egress_policy.purge, crates/modules/connectors/src/handlers.rs:736-747,
    // calls Runtime::purge, crates/kernel/obop-connectors/src/runtime.rs:1455-1493, which deletes FLOW_LOG
    // and SYNC_RUN rows at line 1483). Nobody edits the log: the module registers it read-only
    // (handlers.rs:532-542; crates/kernel/obop-commands/src/store.rs:358-371 gives such a model its reads,
    // list and search, and nothing that writes).
    modelText: {
      data_flow_log:
        "Record of every flow through a connection, written by obop and not edited by people: when, what, how many, which fields, how treated, and a hash of the payload; never the payload. The purge removes entries older than the retention of their connection.",
    },
    // The module generates create, update and delete for three models only (connection, provider_policy,
    // consent: handlers.rs:528-531); the other six are read-only (handlers.rs:532-542), and consent.delete
    // is generated but always refused ("consent records are kept as evidence and cannot be deleted",
    // crates/modules/connectors/src/guards.rs:696-698).
    noDelete: [
      "connector_provider",
      "sync_run",
      "data_flow_log",
      "consent",
      "processing_register",
      "egress_policy",
      "privacy_preview",
    ],
  },
  {
    id: "projects",
    category: "people",
    name: "Project",
    tagline: "Projects and tasks",
    purpose:
      "Plan work as projects, milestones and tasks, with people assigned and deadlines. Progress is tracked by status, which keeps it simple: it suits small teams that want a shared board rather than detailed effort accounting.",
    // projects_task has no [model.access] (packages/projects.toml:161-): a task without a project is
    // read by everyone who reads the model, so it is not "private".
    fieldText: {
      "projects_task.project_id":
        "The project this task belongs to; empty for a task that belongs to no project.",
    },
  },
  {
    id: "helpdesk",
    category: "customers",
    name: "Helpdesk",
    tagline: "Support tickets",
    purpose:
      "Keep customer support requests and work through them, with priorities and response-time targets. An AI agent can read new tickets and set their category and priority, within its permissions. Closing a ticket is a manager's step that an AI agent can only propose.",
    // Category and priority are fields of a ticket that the ticket's write group may set
    // (packages/helpdesk.toml:94-107, model at 57-60); closing is for helpdesk.manager and needs approval
    // (packages/helpdesk.toml:178-186). The module holds no assessment and no reply: a ticket has a
    // description, a resolution and dates, and nothing drafts text.
    modelNames: {
      helpdesk_sla: "Response times",
    },
    // A flag nothing reads: the only asserts in the packages are accounting.toml:346, pos.toml:226 and
    // sign.toml:202, and a search of crates/ and web/apps/backoffice/src finds no reader of it.
    // sla_id is a plain ref (packages/helpdesk.toml:120-125) and due_on a plain date (:139-143); the package has
    // no assert, effect or computed field, and nothing in crates/ reads a ticket outside the tests.
    fieldText: {
      "helpdesk_sla.active": "Marks the policy as in use or not. A label only; nothing checks it.",
      "helpdesk_ticket.sla_id":
        "The service-level policy the ticket is meant to meet. Nothing measures the ticket against it, and nothing fills in the due date.",
    },
    cards: {
      helpdesk_sla: {
        text: "How quickly tickets of each priority should be answered and solved.",
      },
    },
  },
  {
    id: "fleet",
    category: "operations",
    name: "Fleet",
    tagline: "Vehicles and drivers",
    purpose:
      "Keep a list of company vehicles, who drives them and when they were serviced. It records each vehicle's status and services; it does not work out running costs.",
    cards: {
      "fleet_vehicle.retire": {
        text: "Mark an available vehicle as retired. No step moves it back. Needs the approving role.",
      },
      fleet_vehicle_driver: {
        text: "A driver of fleet vehicles: name, phone, licence number and licence expiry date.",
        limit: "A driver's status and licence expiry are not checked when a vehicle is assigned.",
      },
      "fleet_vehicle_driver.suspend": {
        text: "Mark a driver as suspended, for example after an expired licence. By itself, this does not stop the driver being assigned a vehicle.",
      },
      "fleet_vehicle_driver.reactivate": {
        text: "Mark a suspended driver as active again.",
      },
    },
    // The retire action only sets the state to retired and needs fleet.approver (packages/fleet.toml:91-100);
    // no action leaves "retired", and like every model a vehicle has a delete command, so "the record stays"
    // is not said. No action reaches "maintenance" either (:77-100), and a governed state
    // starts at its first option and moves only through actions (crates/kernel/obop-commands/src/guards.rs:65, :158).
    // A driver's state (active, suspended) only moves through suspend and reactivate (:134-154): nothing reads
    // it or licence_expiry, fleet_vehicle.driver_id is a plain ref (:63-68) and dispatch has no assert or effect
    // (:77-82), so the manifest's "authorised to drive" and "allow ... to drive again" (:102, :151) are not said.
    fieldText: {
      "fleet_vehicle.state":
        "Availability: available, in service or retired. The option maintenance exists, but no step moves a vehicle into it.",
    },
  },
  {
    id: "inventory",
    category: "operations",
    name: "Inventory",
    tagline: "Stock and warehouses",
    purpose:
      "Describe your warehouses and locations, record stock levels and log stock moves. When a stock manager confirms a move, the stock is taken from the place it left and added to the place it arrived. Counting is still done by people.",
    modelNames: {
      inventory_move: "Stock move",
    },
    // validate subtracts at the source and adds at the destination, with no check that the source holds
    // enough (packages/inventory.toml:190-210; the package text allows negative quantities, :5). No action
    // leaves "done" (:181-189), but inventory_move is written by inventory.user (:129) and has no lock, so a
    // done move's record can be edited or deleted and the stock does not follow.
    cards: {
      "inventory_move.validate": {
        text: "Confirm that the goods moved: the quantity leaves the source item and arrives at the destination item (stock managers only). Nothing checks that the source holds enough, so stock can go below zero. No action reverses a done move, but its record can still be edited or deleted, and the stock does not follow.",
      },
    },
  },
  {
    id: "purchase",
    category: "operations",
    name: "Purchase",
    tagline: "Vendors and purchase orders",
    purpose:
      "Keep your vendors and the purchase orders you place with them, with order lines and a flow from draft to received. The order total is added up from its lines. A received order can still be edited; stock moves already prepared do not follow.",
    cards: {
      "purchase_order.send": {
        text: "Mark the order as sent to the vendor. By itself, this does not contact the vendor.",
      },
      "purchase_order.confirm": {
        text: "Mark the order as confirmed with the vendor. By itself, this does not contact the vendor.",
      },
      "purchase_order.receive": {
        text: "Record that the goods arrived. You need permission to use Inventory. If the order has a receiving location, a draft stock move is prepared for each line, for the warehouse to confirm.",
      },
    },
  },
  {
    id: "manufacturing",
    category: "operations",
    name: "Manufacturing",
    tagline: "Bills of materials",
    purpose:
      "Define what a product is made of and plan manufacturing orders from draft to done. An order goes through draft, confirmed, in progress and done. The stock of the components is not used up automatically.",
    // A flag nothing reads: the only asserts in the packages are accounting.toml:346, pos.toml:226 and
    // sign.toml:202, and a search of crates/ and web/apps/backoffice/src finds no reader of it.
    fieldText: {
      "manufacturing_bom.active":
        "Marks the recipe as in use or not. A label only; nothing checks it.",
    },
    modelNames: {
      manufacturing_bom: "Bill of materials",
      manufacturing_bom_line: "Component",
    },
  },
  {
    id: "hr",
    category: "people",
    name: "HR",
    tagline: "Employees and leave",
    purpose:
      "A register of employees, departments and leave requests, with strict rules about who sees what. Pay, bank details and national ID number are visible to HR managers only, and health or union data only to the people given that access. AI agents never see any of these. The instance administrator sees all of it.",
    // The manifest's text (packages/hr.toml:5) with the administrator added: platform.admin holds every group
    // (crates/kernel/obop-commands/src/lib.rs:105-118) and the row rule does not restrict an administrator
    // (crates/kernel/obop-commands/src/engine/access.rs:57-59). The leave reason is a confidential_hr text
    // field of hr.manager (packages/hr.toml:270-276), so a health detail typed there is seen by every HR manager.
    // Offboarded is a state no action leaves (:199-213, and guards.rs:158); request only changes the state
    // (:302-308), nothing notifies anyone.
    fieldText: {
      "hr_leave_request.reason":
        "Why the leave is requested. Free text: every HR manager sees it, whatever it says, a health detail included.",
    },
    packageText:
      "Employees, departments and leave requests. Personal data is classified strictly: pay, bank and national id are visible to managers only, and health and union data to the special reader group; the instance administrator, who holds every group, sees all of it. An employee sees and files only their own leave requests; hr.manager and the instance administrator see all of them.",
    cards: {
      "hr_employee.offboard": {
        text: "Mark an active employee as offboarded. The record and its details stay, and no step moves it back.",
      },
      "hr_leave_request.request": {
        text: "Mark the draft as requested, for a manager to approve or refuse. By itself, this does not notify anyone.",
      },
    },
  },
  {
    id: "timesheets",
    category: "people",
    name: "Timesheets",
    tagline: "Time and approval",
    purpose:
      "Record the time spent on project tasks in minutes, mark it as billable and submit it for a manager to approve. By itself, the app does not bill the time or notify anyone. Needs the Projects app.",
    // Submit and approve only change the state (packages/timesheets.toml:85-98); no effect, no assert, and
    // nothing reads timesheets_entry outside the tests (crates/obop/src/main.rs:3412, model_tests.rs:660-667).
    cards: {
      "timesheets_entry.submit": {
        text: "Mark the entry as submitted, for a manager to approve or reject. By itself, this does not notify anyone.",
      },
      "timesheets_entry.approve": {
        text: "Mark a submitted entry as approved (manager only). By itself, this does not bill it.",
      },
    },
  },
  {
    id: "planning",
    category: "people",
    name: "Planning",
    tagline: "Shifts and resources",
    purpose:
      "Plan shifts for your employees. A shift with nobody assigned is an open shift; it goes from open to published, and then a manager records that the employee confirmed it. Templates keep shift patterns, each with a name and start and end times.",
    limits: ["No action creates a shift from a template, and a shift has no field that names one."],
    cards: {
      "planning_shift.publish": {
        text: "Mark the shift as published. By itself, this does not notify anyone: everyone who can read shifts sees every shift, published or not.",
      },
      "planning_shift.confirm": {
        text: "Record that the employee confirmed they will work the shift. Only a planning manager can do this.",
      },
      // A model card is English on a translated page: the product's translation of the template says "to copy into shifts".
      planning_template: {
        text: "A shift pattern, with a name, start and end times and, optionally, a role.",
      },
    },
    // The manifest says the employee confirms ("Workflow: open, published, confirmed by the employee",
    // packages/planning.toml:5; confirm: "The employee confirms they will work the shift", :150), but
    // confirm (name at :149) has no group of its own, so it takes the model's write group planning.manager
    // (:74; the rule is a.group.unwrap_or(&m.write_group), crates/kernel/obop-pkg/src/manifest.rs:1793):
    // an employee with only planning.user cannot run it. Publish (:133) and unpublish (:141) are
    // planning.manager too and have no effect, no notification and no visibility rule: planning_shift
    // has one read group, planning.user (:73), and no [model.access]. Nothing in crates/ or
    // web/apps/backoffice/src refers to planning_template, so a shift is not made from a template.
    // start_minute and end_minute are number fields with no min or max (packages/planning.toml:105-116), and a
    // limit is checked only where one is declared (crates/kernel/obop-commands/src/kinds.rs:146-152).
    // The manifest calls a template "a reusable shift pattern to copy into shifts" (packages/planning.toml:39), which
    // reads as a function; this site says what it keeps (name, start and end, and role_id, which is optional, :49-54).
    modelText: {
      planning_template:
        "A shift pattern, with a name, start and end times and, optionally, a role",
    },
    fieldText: {
      "planning_shift.start_minute":
        "Start in minutes since midnight (540 is 09:00). Meant to be 0 to 1439; nothing checks it.",
      "planning_shift.end_minute":
        "End in minutes since midnight (1020 is 17:00). Meant to be 0 to 1439; nothing checks it, or that the end is after the start.",
    },
    packageText:
      "Shifts for employees, with roles and shift templates (a name, start and end times, optionally a role). A shift has a day and a start and end in minutes since midnight (meant to be 0 to 1439; 540 is 09:00; nothing checks the range, or that the end is after the start). A shift with no employee is an open shift. Workflow: open, published, confirmed; a planning manager moves a shift through all three steps. Needs the Human Resources package.",
  },
  {
    id: "field_service",
    category: "operations",
    name: "Field Service",
    tagline: "Jobs on site",
    purpose:
      "Schedule on-site jobs for customers: who does the work, where, when, which parts were used and a note of whether the customer is to sign off. Site addresses and contacts are marked as personal data, so an AI agent sees them only as placeholders.",
    // The complete action has no assert and no effect (packages/field_service.toml:168-175), and
    // nothing in crates/ or web/apps/backoffice/src reads signature_required: a job is finished
    // whether or not it is set. The package text is the manifest's (line 5) with one clause changed:
    // it says "whether the customer must sign off".
    packageText:
      "On-site jobs for customers, done by a technician: scheduled, en route, in progress, done or cancelled. A job records the customer, the site, the technician, the planned day and start (minutes since midnight), the parts used and a note of whether the customer is to sign off. Site address and contact details are personal data. Needs the Human Resources and Products packages.",
    fieldText: {
      "field_service_job.signature_required":
        "Whether the customer is to sign off the work. A reminder only: a job can be finished without it.",
    },
    cards: {
      "field_service_job.complete": {
        text: "Finish the job. If the customer is to sign off, write down who signed and when.",
      },
    },
  },
  {
    id: "pos",
    category: "operations",
    name: "Point of Sale",
    tagline: "Shop tills",
    purpose:
      "Keep track of a shop's cash register: open and close a shift at the till, and record each sale and how it was paid. It records what happened at the register. It is not connected to card terminals. Do not type card numbers into any field: obop does not check for them.",
    // Card numbers: no field checks for one (provider_reference in packages/pos.toml:340-345 is a plain text
    // field, and nothing in crates/ looks for a card number). The package text is the manifest's (:5) with two changes: the
    // card-number sentence, and "cash-controlled" left out of "cash-controlled sessions", because cash_control is a
    // flag that nothing reads (:44-48; fieldText below). Close has no assert and no counted-cash field is required (:142-148, :116-118); pay
    // checks the payments once, when it runs (:220-226), and pos_order and its lines stay writable by pos.user
    // (:151-154, :252-255). The stock moves are made only when the sale has a stock location (:228-233,
    // when = "is_set(stock_location_id)"), and the package needs Inventory too (:16-19).
    packageText:
      "Shop tills: a point-of-sale configuration, sessions that are opened and closed, orders with lines, and payment lines per method. A refund and the closing of a session need a human approval from a manager; agents may only propose them. Line subtotals and the order total are computed from the lines, receipts get a gapless yearly number, and paying is refused until the payments cover the total; change and cash differences are still entered. When the sale has a stock location, paying also creates a draft stock move per line from it, for the warehouse to validate. Do not store card numbers: obop does not check for them. Keep only the payment provider's reference. Needs the Products and Inventory packages.",
    modelNames: {
      pos_config: "Point of sale",
    },
    // Flags nothing reads: the only asserts in the packages are accounting.toml:346, pos.toml:226 and
    // sign.toml:202, and a search of crates/ and web/apps/backoffice/src finds no reader of them (active:
    // pos.toml:62-66; a session's config_id is a plain ref, :75-79). Nothing prints a till receipt:
    // receipt_header, receipt_number and a line's description are read by no code outside the tests
    // (crates/obop/src/model_tests.rs:462); the shop's printable receipt is eCommerce's order page.
    fieldText: {
      "pos_config.cash_control":
        "Whether sessions are meant to count the cash in the drawer when they open and close. A label only; nothing checks it.",
      "pos_config.allow_refunds":
        "Whether cashiers are meant to propose refunds at this point of sale. A label only; nothing checks it.",
      "pos_config.active":
        "Marks the point of sale as in use or not. A label only; nothing checks it.",
      "pos_config.receipt_header":
        "Text meant for the top of receipts. Point of Sale does not print receipts; the text is only stored.",
      "pos_order.receipt_number":
        "Number for the receipt, such as R-2026-000042: a gapless series that starts again each year, assigned when the sale is created. Point of Sale does not print receipts.",
      "pos_order_line.description":
        "Text meant for the receipt line. Point of Sale does not print receipts; the text is only stored.",
    },
    cards: {
      // The manifest says "and its rules" (packages/pos.toml:23): its only rule-like fields are the two flags
      // above, which nothing reads, and the one assert is on pos_order.pay (:226).
      pos_config: {
        text: "A point of sale: a till or a shop, with its name, currency and receipt header.",
      },
      "pos_order.pay": {
        text: "Mark the sale as fully paid. Refused until the payments cover the total; a paid sale can still be edited afterwards. If the sale has a stock location, a draft record of the goods leaving is prepared for each line, for the warehouse to confirm. You need access to Inventory.",
        limit: "The payments are checked only when the sale is paid.",
      },
      "pos_order.refund": {
        text: "Record that the sale was refunded. By itself, this does not pay money back.",
      },
      "pos_session.close": {
        text: "Close the session (manager only).",
        limit: "obop does not check that the drawer was counted.",
      },
    },
  },
  {
    id: "ecommerce",
    category: "operations",
    name: "eCommerce",
    tagline: "Online store",
    purpose:
      "Keep the data of an online shop: listings, shipping methods, discount codes and web orders. Orders follow a fulfilment flow, and refunds are for managers; an AI agent can only propose them. When the public site is turned on, obop shows the shop to visitors and takes their orders.",
    // No `technical`: it repeated the last sentence of the purpose and added "a custom storefront can use the
    // API instead", which nothing supports (the shop path PublicApi serves obop's own public site only,
    // docs/STOREFRONT.md:127; an order written through the API is not priced, packages/ecommerce.toml:591-605;
    // the headless API is Open in docs/PRODUCT.md).
    // The manifest's text (packages/ecommerce.toml:5) with the stock sentence narrowed: the checkout takes
    // stock only for block_when_out and allow_backorder (crates/kernel/obop-site/src/checkout.rs:318-322, :1207).
    packageText:
      "An online shop: categories, product listings with translations, shipping methods, discount codes, web orders with lines and a fulfilment flow of pending, paid, shipped, delivered, cancelled and refunded. When the instance runs with the public site enabled, visitors browse the published listings and place orders without an account; the server prices every order from the published listing and never trusts the browser. At checkout the server takes the ordered units off each listing whose stock policy is block_when_out or allow_backorder and applies discount codes and tax rates; orders are paid by bank transfer, by card through Stripe Checkout or in a local demo mode; the payment record, the provider reference and refunds are kept with the order. Refunds need a human approval from a manager; agents may only propose them. Customer name, email and addresses are personal data and are never public. Needs the Products package.",
    cards: {
      // refund runs only from paid (packages/ecommerce.toml:665-672); a shipped order has no refund and no
      // cancel (:644-663), and refund_delivered starts from delivered (:674-681).
      "ecommerce_order.refund": {
        text: "Record the refund of an order that is paid and not yet shipped. For card payments through Stripe, obop asks Stripe to refund the payment first, and the order stays paid if Stripe refuses. Otherwise you pay it back yourself.",
        limit:
          "The refund through Stripe is tested only against a local stand-in, not against Stripe itself.",
      },
      "ecommerce_order.refund_delivered": {
        text: "Record the refund of a delivered order, for example after a return. The units are not put back on the shelf. For card payments through Stripe, obop asks Stripe to refund the payment first, and the order stays paid if Stripe refuses. Otherwise you pay it back yourself.",
        limit:
          "The refund through Stripe is tested only against a local stand-in, not against Stripe itself.",
      },
      // The manifest says "computed on the server" (packages/ecommerce.toml:418): the only reader is the public
      // checkout (Site::tax_rates, crates/kernel/obop-site/src/checkout.rs:142-170), which picks a rate by the
      // delivery country and the line's category; an order made by staff is not taxed from these rates.
      ecommerce_tax_rate: {
        text: "A tax rate that the shop's public checkout applies when it prices an order, by delivery country and category.",
      },
    },
    // Units go back only for an order whose checkout recorded a reservation (crates/kernel/obop-payments/src/lib.rs:502-510;
    // docs/DEPLOY.md:139), and the refund of a delivered order never restocks (lib.rs:843-860). Only a listing
    // whose policy is block_when_out or allow_backorder is counted (crates/kernel/obop-site/src/checkout.rs:318-322,
    // :1207; obop-payments/src/lib.rs:530-533). The order number is a required text field (packages/ecommerce.toml:475-479;
    // packages can declare sequences, pos.toml:168), and the checkout makes a random one (obop-site/src/shop.rs:687-697).
    // Free delivery is judged at the checkout on the lines at listing prices, at or above the threshold
    // (checkout.rs:352, :403; pricing.rs:140), and a tax rate is either inside the price or added on top (pricing.rs:152-167).
    fieldText: {
      "ecommerce_listing.stock_quantity":
        "Units on hand, used with the stock policy. For a listing set to block when out or allow backorder, orders placed through the shop's checkout take their units off, and a cancelled, expired or refunded (not yet delivered) order puts them back. A staff-made order, or the refund of a delivered order, does not; correct the quantity by hand, also after a stock count",
      "ecommerce_listing.price":
        "Shop price (minor currency units, e.g. cents). At checkout, a tax rate that applies is either included in it or added on top, as the rate is set",
      "ecommerce_shipping_method.free_over":
        "At the shop's checkout, delivery is free when the lines, at listing prices, come to this amount or more (minor currency units); empty means never",
      "ecommerce_order.number":
        "Order number shown to the shopper. A required text field: the public checkout makes a random one, and any other order needs one entered.",
    },
  },
  {
    id: "website",
    category: "content",
    name: "Website",
    tagline: "Pages and contact form",
    purpose:
      "Hold the content of a public website: pages, menus, blog posts and the messages visitors send through the site's contact form. This app stores and organises the content; with the public site turned on, obop shows it to visitors. Publishing a page or a blog post is for managers; an AI agent can only propose it.",
    cards: {
      website_page: {
        text: "A page of the website. Edits to a published page need no further approval.",
      },
      website_menu_item: {
        text: "An entry of the site's menu. It is shown as soon as its visible box is ticked; anyone who can edit it, an AI agent included, can do that without approval.",
      },
      website_page_translation: {
        text: "A page in another language. It is shown as soon as its published box is ticked, once its page is published; anyone who can edit it, an AI agent included, can do that without approval.",
      },
      website_blog_post_translation: {
        text: "A blog post in another language. It is shown as soon as its published box is ticked, once its post is published; anyone who can edit it, an AI agent included, can do that without approval.",
      },
    },
    // The publish action has no effect (packages/website.toml:84-90, and the package has none), so it does not
    // set published_on (:62-66), which the public site uses as the page's date (obop-site/src/content.rs:244, :253).
    fieldText: {
      "website_page.published_on":
        "The page's publish date (YYYY-MM-DD). Publishing does not fill it in.",
    },
    // Only a page's or a post's state is moved by an action that needs approval (packages/website.toml:84-90,
    // :237-245). The published flag of a translation (:393-398, :443-448) and the visible flag of a menu entry
    // (:145-149) are plain checkbox fields, and the public site shows a record whose flag is set
    // (crates/kernel/obop-commands/src/public.rs:259-263). The only form on the public site is /contact
    // (crates/kernel/obop-site/src/content.rs:696, :742). The package text is the manifest's (:5) with the
    // publishing sentence corrected.
    packageText:
      "Content for a public website: pages with a slug and a draft, published, archived flow, navigation menu items, blog posts with a byline and a publish date, translations of both, and the form submissions visitors send in. When the instance runs with the public site enabled it serves the published content to visitors as a server-rendered website with SEO and no login; only the fields marked public are ever visible to them. Publishing a page or a blog post needs a manager, and an AI agent can only propose it; the published flag of a translation and the visible flag of a menu entry are ordinary fields without that step. Senders are personal data and are never public.",
  },
  {
    id: "email_marketing",
    category: "content",
    name: "Email Marketing",
    tagline: "Campaigns",
    purpose:
      "Keep mailing lists with each contact's consent status and date, and prepare campaigns. By itself, marking a campaign as sent only records that it went out; obop does not send the emails itself. A connection set up in Connectors can pass the campaign to another tool when it is marked as sent.",
    // A connection to a service that receives records delivers the record on the commands it lists
    // (crates/kernel/obop-connectors/src/runtime.rs:758-800, :1580-1605; the events field,
    // crates/kernel/obop-connectors/src/catalog.rs:249-255). The package text says "Sending is irreversible and reaches real people, so it needs a human
    // approval" (packages/email_marketing.toml:5). Nothing in the repository sends an email: the
    // action marks the campaign as sent. The rest of the text is the manifest's, word for word.
    packageText:
      "Mailing lists, the contacts on them with their consent status and date, and campaigns with a draft, scheduled, sent flow. Marking a campaign as sent needs a manager, and agents may only propose it; obop does not send the emails itself. Campaign statistics are numbers entered or imported, not computed. Contacts are personal data: keep consent_status and consent_date accurate and never mail a contact that is not opted_in.",
    // email_marketing_list.is_public: packages/email_marketing.toml has no [model.public] section and
    // nothing in crates/ or web/apps/backoffice/src reads the flag.
    fieldText: {
      "email_marketing_list.is_public":
        "A label only; nothing checks it, and this app has no sign-up page for visitors.",
    },
    cards: {
      email_marketing_campaign: {
        text: "One mailing to a list: its subject, sender, content and the day it should go out. By itself, obop does not send it.",
      },
      "email_marketing_campaign.schedule": {
        text: "Mark the campaign as scheduled for its day. By itself, this does not send it.",
      },
      "email_marketing_campaign.send": {
        text: "Record that the scheduled campaign has been sent (manager only). obop does not send the emails itself.",
      },
      "email_marketing_campaign.send_now": {
        text: "Record that a draft campaign has been sent (manager only). obop does not send the emails itself.",
      },
    },
  },
  {
    id: "documents",
    category: "content",
    name: "Documents",
    tagline: "Files and folders",
    purpose:
      "A register of business documents, with their folder, kind, owner, expiry date and confidentiality. You can keep the file itself here, or just a note of where it is kept. An AI agent sees that a file is attached, but never receives the file itself.",
    // An AI agent never reads the bytes of a stored file (crates/kernel/obop-commands/src/blobs.rs:247-251;
    // crates/kernel/obop-mcp/src/schema.rs:49). The administrator holds every group, so the row rule does not
    // hide a confidential document from the instance administrator (crates/kernel/obop-commands/src/engine/access.rs:1-8, :57-59,
    // crates/kernel/obop-commands/src/lib.rs:105-118). The package text is the manifest's (packages/documents.toml:5)
    // with the administrator added.
    packageText:
      "A register of business documents: folders, document records with kind, owner, expiry and confidentiality, and a draft, published and archived flow. It stores references and, when you upload one, the file itself. Confidentiality limits who can see a document: confidential and restricted documents are visible only to their owner, to documents.manager and to the instance administrator, and the two behave the same; public and internal ones to everyone who may read documents.",
    fieldText: {
      "documents_document.confidentiality":
        "Who may see it: public and internal documents everyone who may read documents, confidential and restricted ones only the owner, documents.manager and the instance administrator (the two behave the same)",
    },
    // The manifest says publish makes a draft "the current version" (packages/documents.toml:132): the
    // action only moves state to published, and the model has no version or history field.
    cards: {
      "documents_document.publish": {
        text: "Mark a draft document as published.",
      },
    },
  },
  {
    id: "sign",
    category: "content",
    name: "Sign",
    tagline: "Simple e-signatures",
    purpose:
      "Collect simple electronic signatures: the signer types their name, which does not prove who they are. Keep templates and signature requests, and follow each request from draft to signed. For a PDF from Documents it keeps an exact copy and a check code. An AI agent cannot create signing links.",
    technical:
      "For a PDF from Documents the app records the SHA-256 hash of the file and keeps an exact copy. A signed request can be changed or deleted only by an administrator, and the record is not tamper-proof: whoever can write the database can change it.",
    modelNames: {
      sign_request: "Signature request",
    },
    // sign_link.delete is refused to everyone but a person in the admin group, so an AI agent's
    // delete is refused even after a person approves it (crates/kernel/obop-commands/src/sign/link.rs:30-40,
    // is_admin at sign/mod.rs:196-198): the page names no such step. The manifest's email field says
    // "obop sends no email" (packages/sign.toml:95); a connection can send one (sign_docuseal).
    noDelete: ["sign_link"],
    fieldText: {
      "sign_request.signer_email":
        "Email address of the signer; optional. By itself, obop sends no email for a request about a document.",
    },
    cards: {
      "sign_request.send": {
        text: "Mark the request as sent. By itself, this does not email the signer.",
      },
      "sign_request.send_document": {
        text: "Send a request about a PDF stored in Documents. obop keeps an exact copy for the signer to read. Then create the signing link.",
      },
      sign_link: {
        text: "A one-time link that lets one person sign one request. It is shown once, when it is created.",
      },
    },
  },
  {
    id: "subscriptions",
    category: "customers",
    name: "Subscriptions",
    tagline: "Plans and subscriptions",
    purpose:
      "Define recurring plans and follow your customers' subscriptions, from trial to cancelled. A subscription is in trial, active, past due or cancelled, and has the date of its next invoice. It does not create invoices by itself.",
    // A flag nothing reads: the only asserts in the packages are accounting.toml:346, pos.toml:226 and
    // sign.toml:202, and a search of crates/ and web/apps/backoffice/src finds no reader of it.
    fieldText: {
      "subscriptions_plan.active":
        "Marks the plan as open to new customers or not. A label only; nothing checks it.",
    },
    cards: {
      "subscriptions_subscription.activate": {
        text: "Mark the subscription as active when the trial ends. By itself, this does not start any billing.",
      },
    },
  },
  {
    id: "dashboards",
    category: "content",
    name: "Dashboards",
    tagline: "Reports and charts",
    purpose:
      "Build dashboards that show your numbers as charts and tables. A dashboard is a set of items: a single number, a bar, line or pie chart, or a table.",
    technical:
      "Dashboards and widgets are stored as data (KPI, bar, line, pie and table definitions) for a client to render. The package never runs a query and does not check that the models a widget names exist.",
    modelNames: {
      dashboards_widget: "Dashboard item",
    },
    cards: {
      dashboards_dashboard: {
        text: "A named set of charts, figures and tables, in the order you choose.",
      },
      dashboards_widget: {
        text: "One chart, figure or table on a dashboard, defined by what it shows and how the numbers are added up.",
      },
    },
  },
  {
    id: "accounting",
    category: "finance",
    name: "Accounting",
    tagline: "Ledger and journals",
    purpose:
      "Record what your business earns and spends, and post each entry to the ledger. An entry is a set of debit and credit lines; its totals are checked only at posting. Closing a month or year is only a label; it does not stop new entries. Tax amounts are typed in by hand.",
    modelNames: {
      accounting_move: "Journal entry",
      accounting_move_line: "Journal entry line",
    },
    // Post, reverse, close and reopen are for accounting.approver (packages/accounting.toml:195, :204, :255, :264, :344, :361).
    // The balance is asserted once, when post runs (:346); the lines of a posted entry are written by
    // accounting.user (:271, :368) and no lock stops them, and an entry without lines has two zero totals. Nothing in
    // crates/ or web/apps/backoffice/src reads reconcile, reconciled, due_on, price_included, tax_account_id or
    // default_account_id (the importers only write reconcile: crates/obop/src/import_oas/mod.rs:924,
    // crates/obop/src/sie/import.rs:507), and a journal's currency is a text field with no check (:99-101).
    // deprecated, active (journal, tax): the only assert in the package is the balance
    // (packages/accounting.toml:346) and nothing in crates/ or web/apps/backoffice/src reads these flags.
    fieldText: {
      "accounting_account.reconcile": "A label only; nothing uses it.",
      "accounting_move_line.reconciled": "A label only; nothing uses it.",
      "accounting_move_line.due_on": "A date you enter; nothing uses it.",
      "accounting_tax.price_included": "A label only; nothing uses it.",
      "accounting_tax.tax_account_id": "A label only; nothing uses it.",
      "accounting_journal.default_account_id": "A label only; nothing uses it.",
      "accounting_journal.currency": "A currency code such as EUR; not checked.",
      "accounting_bank_statement_line.move_line_id":
        "The entry line this statement line was matched to, if you set it. Optional: matching does not require it.",
      "accounting_account.deprecated":
        "A label only: entries can still be posted to a deprecated account.",
      "accounting_journal.active":
        "A label only: entries can still be booked in an inactive journal.",
      "accounting_tax.active": "A label only: an inactive tax can still be put on a line.",
    },
    cards: {
      "accounting_fiscal_year.close": {
        text: "Mark the fiscal year as closed (needs the approving role). This is a label only: entries can still be posted into it.",
      },
      "accounting_fiscal_year.reopen": {
        text: "Mark a closed fiscal year as open again (needs the approving role).",
      },
      "accounting_period.close": {
        text: "Mark the period as closed (needs the approving role). This is a label only: entries can still be posted into it.",
      },
      "accounting_period.reopen": {
        text: "Mark a closed period as open again (needs the approving role).",
      },
      "accounting_move.post": {
        text: "Post a draft entry to the ledger and give it its next number (needs the approving role). Refused unless the debits equal the credits when you post. A posted entry can still be edited afterwards.",
        limit:
          "An entry with no lines has debits and credits of zero, which are equal, so it can be posted.",
      },
      "accounting_move.reverse": {
        text: "Mark a posted entry as reversed (needs the approving role). The opposite entry is booked separately.",
      },
    },
  },
  {
    id: "knowledge",
    category: "content",
    name: "Knowledge",
    tagline: "Articles and guides",
    purpose:
      "A shared handbook of articles. Your team writes, arranges in a tree and tags them; managers publish, unpublish and archive them. Visibility is only a label: this package sets no per-article permissions, so everyone who can open the app reads every article, drafts included.",
    // The platform has row-level rules (crates/kernel/obop-commands/src/engine/access.rs:1-8, used by
    // packages/hr.toml, timesheets.toml, documents.toml and dashboards.toml); this package declares none (it has no
    // [model.access]), so everyone with its groups reads every article. The package text is the manifest's
    // (packages/knowledge.toml:5) without the claim about the platform. knowledge.user reads and writes
    // articles (:19-20); publish, unpublish and archive are knowledge.manager (:80-102).
    packageText:
      "A wiki: articles arranged in a tree through a parent article, with a draft, published, archived flow, a visibility setting and tags. Visibility is a label for the UI and for people to respect; this package sets no row rules, so everyone with the Knowledge groups can read every article.",
  },
  {
    id: "appointments",
    category: "customers",
    name: "Appointments",
    tagline: "Bookable resources",
    purpose:
      "Describe the people, rooms and equipment that can be booked, and record appointments with a date, a start and an end. Overlapping bookings are not prevented automatically.",
    // A flag nothing reads: the only asserts in the packages are accounting.toml:346, pos.toml:226 and
    // sign.toml:202, and a search of crates/ and web/apps/backoffice/src finds no reader of it.
    fieldText: {
      "appointments_resource.active":
        "Marks the resource as bookable or not. A label only; nothing checks it.",
      // packages/appointments.toml:36-40; the package has no assert or effect, and nothing in crates/ reads it.
      "appointments_resource.capacity":
        "How many people it can serve at once. A note; nothing checks it.",
    },
  },
  {
    id: "expenses",
    category: "finance",
    name: "Expenses",
    tagline: "Claims and approvals",
    purpose:
      "Employees file expense claims that move from draft to submitted, approved and marked paid. Marking a claim paid is for the finance role; an AI agent can only propose it. By itself, the app does not pay anyone. Receipts are mentioned in a note, not stored.",
    // The four actions only change the state; the pay action says "Record the reimbursement as paid"
    // (packages/expenses.toml:104-135). There is no assert and no lock; the access rule only filters what an
    // employee sees (:22-24), and the approve group is expenses.manager with no check of whose claim it is.
    cards: {
      "expenses_expense.approve": {
        text: "Approve a submitted claim (manager only). This does not lock the claim, and a manager can approve their own.",
      },
      "expenses_expense.pay": {
        text: "Mark an approved claim as paid (finance only). By itself, this does not pay anyone, and a paid claim can still be changed or deleted by the employee.",
      },
    },
  },
];
