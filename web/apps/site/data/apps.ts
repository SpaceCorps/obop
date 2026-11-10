/**
 * Hand-written copy per app. Everything structural (models, fields, workflows, groups, approvals) is
 * read from packages/*.toml and data/builtin/*.toml at build time; only the human "what it is for"
 * text lives here. English only; the one-line taglines are translated in i18n/<locale>.ts.
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
}

export interface CardCopy {
  /** The card's heading, when the words of its action name read oddly. */
  title?: string;
  text: string;
}

export const BUILTIN_IDS = ["base", "product", "crm", "sales", "invoicing", "connectors"] as const;

export const APP_COPY: AppCopy[] = [
  {
    id: "crm",
    category: "customers",
    name: "CRM",
    tagline: "Follow your sales leads",
    purpose:
      "Keep track of possible sales, from the first contact until you win or lose them. Each lead has a title, a contact name and email, the revenue you expect and a stage. Moving a lead through the steps of a sale is a single action that your team and its AI assistants can both take.",
  },
  {
    id: "sales",
    category: "customers",
    name: "Sales",
    tagline: "Quotations and orders",
    purpose:
      "Write quotes for customers and turn them into confirmed orders. Each order belongs to a contact and has lines, a total and a currency. Confirming an order prepares a draft invoice with the same lines, for Invoicing to finalise. The person who confirms also needs access to Invoicing.",
  },
  {
    id: "invoicing",
    category: "finance",
    name: "Invoicing",
    tagline: "Bill your customers",
    purpose:
      "Prepare invoices for your customers as drafts, then finalise each one to give it its own number, in a yearly series with no gaps. Finalising needs a person with the right permission: an AI assistant can prepare the draft and propose finalising it, but cannot finalise it.",
    cards: {
      invoice: {
        text: "A customer invoice: its lines, a total that adds them up, and a number once it is posted.",
      },
      "invoice.post": {
        text: "Finalise a draft invoice and give it its next number in the yearly series.",
      },
    },
    // The module says "posted invoices are corrected only by credit notes" (crates/modules/invoicing/src/lib.rs:17):
    // there is no credit note in the module, and nothing locks a posted invoice's fields. What holds is
    // that no action leaves "posted". Its package text says "legal number"; the module gives a gapless
    // number of the year and makes no claim about the law.
    packageText:
      "Customer invoices with lines: drafts, posting with a gapless yearly number, cancellation.",
    modelText: {
      invoice: "A customer invoice. A posted invoice cannot go back to draft.",
    },
  },
  {
    id: "product",
    category: "foundation",
    name: "Products",
    tagline: "Your products and prices",
    purpose:
      "The list of everything you sell, with its name, product code, price and cost. Cost is shown only to people who may see costs, so a sales assistant can quote a price without seeing your margins.",
  },
  {
    id: "base",
    category: "foundation",
    name: "Contacts",
    tagline: "People and companies",
    purpose:
      "One record for every person and company you deal with, used by Sales, Invoicing, Purchase and other apps. Names and email addresses are personal data, so AI assistants see only part of them, and the tax number is kept from them completely. A separate erase step, for the privacy officer role, replaces a contact's details with [erased].",
    technical:
      "The erase action anonymises a contact in place and is gated by approval. Names and emails are masked for agents; the tax identifier is withheld.",
    modelNames: {
      party: "Contact",
    },
    cards: {
      "party.erase": {
        text: "Replace a contact's name, email, tax number and notes with [erased], and remove the photo. Copies in other apps are not touched.",
      },
    },
  },
  {
    id: "connectors",
    category: "foundation",
    name: "Connectors",
    tagline: "Connect your other tools",
    purpose:
      "Connect your other tools to obop, with privacy controls built in. These are technical controls that support GDPR compliance, not legal advice or a certification. The catalogue says for every service how much of it is built. Nothing goes out or comes in until the reason for sharing is written down and the details that may be shared are listed. Personal details go out under a code name (a pseudonym) unless you decide otherwise, and sensitive details stay back. Every transfer is logged without its content. When someone withdraws their consent, the service is disconnected and its sign-in details are deleted. A list of what each connection processes is also kept, and you can export it as a starting point for your own record of processing (GDPR Article 30).",
    technical:
      "Personal data leaves as keyed pseudonyms by default, the sensitive data classes stay withheld, every flow is recorded without its payload, and withdrawing consent disconnects the service and purges its credentials. The processing register is a generated read model over the connections, providers and consents, exportable as CSV; the complete Article 30 record stays with the operator.",
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
      "connection.revoke": {
        text: "End the connection for good. Access stops, and the sign-in details and code-name lists are deleted.",
      },
      "consent.withdraw": {
        text: "Withdraw the reason for sharing: the connection ends and its sign-in details are deleted.",
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
        text: "A record of every transfer that is only ever added to: when, what, how many, which details, how they were treated, and a fingerprint of the content. Never the content itself.",
      },
      processing_register: {
        text: "A generated list of what each connection processes, as a starting point for your own record of processing under GDPR; exportable as CSV.",
      },
      egress_policy: {
        text: "Switches for the whole instance: the master switch, EU-only services, whether an agreement is required, and the allowed hosts.",
      },
      privacy_preview: {
        text: "What a sample record looks like after the privacy rules, so you can check it before anything is sent.",
      },
    },
  },
  {
    id: "projects",
    category: "people",
    name: "Project",
    tagline: "Projects and tasks",
    purpose:
      "Plan work as projects, milestones and tasks, with people assigned and deadlines. Progress is tracked by status, which keeps it simple: it suits small teams that want a shared board rather than detailed effort accounting.",
  },
  {
    id: "helpdesk",
    category: "customers",
    name: "Helpdesk",
    tagline: "Support tickets",
    purpose:
      "Keep customer support requests and work through them, with priorities and response-time targets. A typical use is an AI assistant that reads new tickets and drafts a first assessment, while people keep ownership of the replies.",
    modelNames: {
      helpdesk_sla: "Response times",
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
        text: "Remove an available vehicle from the fleet for good.",
      },
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
  },
  {
    id: "purchase",
    category: "operations",
    name: "Purchase",
    tagline: "Vendors and purchase orders",
    purpose:
      "Keep your vendors and the purchase orders you place with them, with order lines and a flow from draft to received. The order total is added up from its lines.",
    cards: {
      "purchase_order.send": {
        text: "Mark the order as sent to the vendor. obop does not contact the vendor.",
      },
      "purchase_order.confirm": {
        text: "Mark the order as confirmed with the vendor. obop does not contact the vendor.",
      },
      "purchase_order.receive": {
        text: "Record that the goods arrived. A draft stock move is prepared for each line, for the warehouse to confirm. Needs permission to use Inventory.",
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
      "A register of employees, departments and leave requests, with strict rules about who sees what. Pay, bank details and national ID number are visible to managers only, and health or union data only to the people given that access. AI assistants never see any of these.",
  },
  {
    id: "timesheets",
    category: "people",
    name: "Timesheets",
    tagline: "Time and approval",
    purpose:
      "Record the time spent on project tasks in minutes, mark it as billable and send it for approval. Needs the Projects app.",
  },
  {
    id: "planning",
    category: "people",
    name: "Planning",
    tagline: "Shifts and resources",
    purpose:
      "Plan shifts for your employees from reusable templates. A shift with nobody assigned is an open shift; it goes from open to published, and then the employee confirms it.",
  },
  {
    id: "field_service",
    category: "operations",
    name: "Field Service",
    tagline: "Jobs on site",
    purpose:
      "Schedule on-site jobs for customers: who does the work, where, when, which parts were used and whether the customer must sign off. Site addresses and contacts are personal data and are treated as such.",
    cards: {
      "field_service_job.complete": {
        text: "Finish the job. If the customer must sign off, write down who signed and when first.",
      },
    },
  },
  {
    id: "pos",
    category: "operations",
    name: "Point of Sale",
    tagline: "Shop tills",
    purpose:
      "Keep track of a shop's cash register: open and close the cash drawer, and record each sale and how it was paid. It records what happened at the register. It is not connected to card terminals, and card numbers must never be stored.",
    modelNames: {
      pos_config: "Point of sale",
    },
    cards: {
      "pos_order.pay": {
        text: "Mark the sale as fully paid. Refused until the payments cover the total. If the sale has a stock location, a draft record of the goods leaving is prepared for each line, for the warehouse to confirm. You need access to Inventory.",
      },
      "pos_order.refund": {
        text: "Record that the sale was refunded. obop does not pay money back.",
      },
    },
  },
  {
    id: "ecommerce",
    category: "operations",
    name: "eCommerce",
    tagline: "Online store",
    purpose:
      "Keep the data of an online shop: listings, shipping methods, discount codes and web orders. Orders follow a fulfilment flow, and refunds are for managers; an AI assistant can only propose them. When the public site is turned on, obop shows the shop to visitors and takes their orders.",
    technical:
      "With the public site turned on, obop shows the shop to visitors and takes orders; a custom storefront can use the API instead.",
    cards: {
      "ecommerce_order.refund": {
        text: "Record the refund of a paid order that was not delivered. For card payments through Stripe the money is returned too; otherwise you pay it back yourself.",
      },
      "ecommerce_order.refund_delivered": {
        text: "Record the refund of a delivered order, for example after a return. For card payments through Stripe the money is returned too; otherwise you pay it back yourself.",
      },
    },
  },
  {
    id: "website",
    category: "content",
    name: "Website",
    tagline: "Pages and forms",
    purpose:
      "Hold the content of a public website: pages, menus, blog posts and the forms visitors send. This app stores and organises the content; with the public site turned on, obop shows it to visitors. Publishing is for managers; an AI assistant can only propose it.",
  },
  {
    id: "email_marketing",
    category: "content",
    name: "Email Marketing",
    tagline: "Campaigns",
    purpose:
      "Keep mailing lists with each contact's consent status and date, and prepare campaigns. Emails are not sent by obop: marking a campaign as sent only records that it went out.",
    // The package text says "Sending is irreversible and reaches real people, so it needs a human
    // approval" (packages/email_marketing.toml:5). Nothing in the repository sends an email: the
    // action marks the campaign as sent. The rest of the text is the manifest's, word for word.
    packageText:
      "Mailing lists, the contacts on them with their consent status and date, and campaigns with a draft, scheduled, sent flow. Marking a campaign as sent needs a manager, and agents may only propose it; obop does not send the emails itself. Campaign statistics are numbers entered or imported, not computed. Contacts are personal data: keep consent_status and consent_date accurate and never mail a contact that is not opted_in.",
    cards: {
      email_marketing_campaign: {
        text: "One mailing to a list: its subject, sender, content and the day it should go out. obop does not send it.",
      },
      "email_marketing_campaign.schedule": {
        text: "Mark the campaign as scheduled for its day. obop does not send it.",
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
      "A register of business documents, with their folder, kind, owner, expiry date and confidentiality. You can keep the file itself here, or just a note of where it is kept.",
  },
  {
    id: "sign",
    category: "content",
    name: "Sign",
    tagline: "E-signatures",
    purpose:
      "Keep templates and signature requests, and follow each request from draft to signed. This is a simple electronic signature: the signer types their name, which does not prove who they are. For a PDF from Documents it keeps an exact copy and a check code. An AI assistant cannot create signing links.",
    technical:
      "For a PDF from Documents the app records the SHA-256 hash of the file and keeps an exact copy. A signed request can be changed or deleted only by an administrator, and the record is not tamper-proof: whoever can write the database can change it.",
    modelNames: {
      sign_request: "Signature request",
    },
    cards: {
      "sign_request.send": {
        text: "Mark the request as sent. obop does not email the signer.",
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
    cards: {
      "subscriptions_subscription.activate": {
        text: "Mark the subscription as active when the trial ends. obop does not start any billing.",
      },
    },
  },
  {
    id: "dashboards",
    category: "content",
    name: "Dashboards",
    tagline: "Reports and charts",
    purpose:
      "Build dashboards that show your numbers as charts and tables. A dashboard is a set of items: a single number, a bar, line or pie chart, or a table. The app stores what each item should show and does not check that the data it points to exists.",
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
      "Record what your business earns and spends, and finalise each entry in the ledger. An entry is a set of debit and credit lines, finalised only when the totals match. Closing a month or year is only a label; it does not stop new entries. Tax amounts are typed in by hand.",
    modelNames: {
      accounting_move: "Journal entry",
      accounting_move_line: "Journal entry line",
    },
    cards: {
      "accounting_fiscal_year.close": {
        text: "Mark the fiscal year as closed. This is a label only: entries can still be posted into it.",
      },
      "accounting_fiscal_year.reopen": {
        text: "Mark a closed fiscal year as open again.",
      },
      "accounting_period.close": {
        text: "Mark the period as closed. This is a label only: entries can still be posted into it.",
      },
      "accounting_period.reopen": {
        text: "Mark a closed period as open again.",
      },
      "accounting_move.post": {
        text: "Finalise a draft entry in the ledger and give it its next number. Refused unless the debits equal the credits.",
      },
      "accounting_move.reverse": {
        text: "Mark a posted entry as reversed. The opposite entry is booked separately.",
      },
    },
  },
  {
    id: "knowledge",
    category: "content",
    name: "Knowledge",
    tagline: "Articles and guides",
    purpose:
      "A shared handbook of articles that your team can write, arrange in a tree, publish and tag. Visibility is only a label: there are no permissions per article yet, so everyone who can open the app reads every article.",
  },
  {
    id: "appointments",
    category: "customers",
    name: "Appointments",
    tagline: "Bookable resources",
    purpose:
      "Describe the people, rooms and equipment that can be booked, and record appointments with a date, a start and an end. Overlapping bookings are not prevented automatically.",
  },
  {
    id: "expenses",
    category: "finance",
    name: "Expenses",
    tagline: "Claims and approvals",
    purpose:
      "Employees file expense claims that move from draft to submitted, approved and paid. Paying is for the finance role; an AI assistant can only propose it. Receipts are mentioned in a note, not stored.",
  },
];
