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
}

export const BUILTIN_IDS = ["base", "product", "crm", "sales", "invoicing", "connectors"] as const;

export const APP_COPY: AppCopy[] = [
  {
    id: "crm",
    category: "customers",
    name: "CRM",
    tagline: "Follow your sales leads",
    purpose:
      "Keep track of possible sales, from the first contact until you win or lose them. Each lead has a title, a contact, the revenue you expect and a stage. Moving a lead along the pipeline is a named step that your team and its AI agents can both take.",
  },
  {
    id: "sales",
    category: "customers",
    name: "Sales",
    tagline: "Quotations and orders",
    purpose:
      "Write quotations for customers and turn them into confirmed orders. Each order belongs to a contact and has a total and a currency. Invoicing bills the confirmed orders.",
  },
  {
    id: "invoicing",
    category: "finance",
    name: "Invoicing",
    tagline: "Bill your customers",
    purpose:
      "Prepare invoices for your customers as drafts, then post them to give each one its own legal invoice number. Posting an invoice always needs a person: an AI agent can prepare the draft and suggest posting it, and someone with the right permission approves it.",
  },
  {
    id: "product",
    category: "foundation",
    name: "Products",
    tagline: "Your products and prices",
    purpose:
      "The list of everything you sell, with its name, product code, price and cost. Cost is shown only to people in the cost viewer group, so a sales assistant or an AI agent can quote a price without seeing your margins.",
  },
  {
    id: "base",
    category: "foundation",
    name: "Contacts",
    tagline: "People and companies",
    purpose:
      "One record for every person and company you deal with, shared by all the other apps. Names and email addresses are personal data, so AI agents see only part of them, and the tax number is kept from them completely. A separate erase step, which a person must approve, removes a contact's personal details.",
    technical:
      "The erase action anonymises a contact in place and is gated by approval. Names and emails are masked for agents; the tax identifier is withheld.",
    modelNames: {
      party: "Contact",
    },
  },
  {
    id: "connectors",
    category: "foundation",
    name: "Connectors",
    tagline: "Connect your other tools",
    purpose:
      "Connect your other tools to obop, with privacy controls built in. Nothing goes out or comes in without a recorded legal basis and a list of the fields that may be shared. Personal data goes out under a code name (a pseudonym) unless you decide otherwise, sensitive data stays back, and every transfer is logged without its content. When someone withdraws their consent, the service is disconnected and its sign-in details are deleted. obop also writes the processing register that Article 30 of the GDPR asks for. These are technical controls that support GDPR compliance: they are not legal advice or a certification, and the catalogue says for every service how much of it is built.",
    technical:
      "Personal data leaves as keyed pseudonyms by default, the sensitive data classes stay withheld, every flow is recorded without its payload, and withdrawing consent disconnects the service and purges its credentials. A processing register for Article 30 is generated from the connections.",
    modelNames: {
      connector_provider: "Service",
      provider_policy: "Service settings",
      egress_policy: "Data sharing settings",
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
      "Receive customer support requests and work through them, with priorities and response-time targets. A typical use is an AI agent that reads new tickets and drafts a first assessment, while people keep ownership of the replies.",
  },
  {
    id: "fleet",
    category: "operations",
    name: "Fleet",
    tagline: "Vehicles and drivers",
    purpose:
      "Keep a list of company vehicles, who drives them and when they were serviced. It records each vehicle's status and services; it does not work out running costs.",
  },
  {
    id: "inventory",
    category: "operations",
    name: "Inventory",
    tagline: "Stock and warehouses",
    purpose:
      "Describe your warehouses and locations, record stock levels and log stock moves. People enter the quantities, so it works as a record of what was counted and moved rather than an automatic stock system.",
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
      "Keep your vendors and the purchase orders you send them, with order lines and a flow from draft to received. Totals are typed in, not added up from the lines.",
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
      "A register of employees, departments and leave requests, with strict rules about who sees what. Pay, bank details and national id are visible to managers only, and health or union data only to a special group of readers. AI agents never see the sensitive kinds.",
  },
  {
    id: "timesheets",
    category: "people",
    name: "Timesheets",
    tagline: "Time and billing",
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
  },
  {
    id: "pos",
    category: "operations",
    name: "Point of Sale",
    tagline: "Retail and restaurants",
    purpose:
      "Model a shop till: its settings, cash-controlled sessions, and orders with their lines and payments by method. It records what happened at the till. It is not a connection to card terminals, and card numbers must never be stored.",
    modelNames: {
      pos_config: "Point of sale",
    },
  },
  {
    id: "ecommerce",
    category: "operations",
    name: "eCommerce",
    tagline: "Online store",
    purpose:
      "Keep the data of an online shop: listings, shipping methods, discount codes and web orders. Orders follow a fulfilment flow, and refunds need a manager and an approval.",
    technical:
      "A storefront or another client reads and writes this data through the API; the package does not render a shop.",
  },
  {
    id: "website",
    category: "content",
    name: "Website",
    tagline: "Pages and forms",
    purpose:
      "Hold the content of a public website: pages, menus, blog posts and the forms visitors send. This app stores and organises the content, and something else shows it. Publishing needs approval.",
  },
  {
    id: "email_marketing",
    category: "content",
    name: "Email Marketing",
    tagline: "Campaigns",
    purpose:
      "Keep mailing lists with each contact's consent status and date, and prepare campaigns. Sending reaches real people and cannot be undone, so it always needs a person's approval.",
  },
  {
    id: "documents",
    category: "content",
    name: "Documents",
    tagline: "Files and folders",
    purpose:
      "A register of business documents, with their folder, kind, owner, expiry date and confidentiality. It keeps a reference to each document, not the file itself.",
  },
  {
    id: "sign",
    category: "content",
    name: "Sign",
    tagline: "E-signatures",
    purpose:
      "Keep templates and signature requests, and follow each request from draft to signed. A request goes through draft, sent, signed, declined and expired, and the app records a fingerprint of the document that was signed. The platform cannot yet lock a signed request, so treat it as final by agreement only.",
    technical:
      "The app records the hash of the signed document. The platform cannot yet lock a record, so a signed request is immutable by convention only.",
    modelNames: {
      sign_request: "Signature request",
    },
  },
  {
    id: "subscriptions",
    category: "customers",
    name: "Subscriptions",
    tagline: "Recurring billing",
    purpose:
      "Define recurring plans and follow your customers' subscriptions, from trial to cancelled. A subscription is in trial, active, past due or cancelled, and has the date of its next invoice. It does not create invoices by itself.",
  },
  {
    id: "dashboards",
    category: "content",
    name: "Dashboards",
    tagline: "Reports and charts",
    purpose:
      "Build dashboards that show your numbers as charts and tables. A dashboard is a set of widgets: a single figure, a bar, line or pie chart, or a table. The app stores what each widget should show and does not check that the data it points to exists.",
    technical:
      "Dashboards and widgets are stored as data (KPI, bar, line, pie and table definitions) for a client to render. The package never runs a query and does not check that the models a widget names exist.",
  },
  {
    id: "accounting",
    category: "finance",
    name: "Accounting",
    tagline: "Ledger and reports",
    purpose:
      "Keep your books: accounts, journals, tax rates, journal entries, fiscal periods and bank statement lines. Journal entries have debit and credit lines. Nothing is calculated for you: whoever enters the figures is responsible for balancing them and for the tax amounts. Posting and closing need approval.",
    modelNames: {
      accounting_move: "Journal entry",
      accounting_move_line: "Journal entry line",
    },
  },
  {
    id: "knowledge",
    category: "content",
    name: "Knowledge",
    tagline: "Articles and guides",
    purpose:
      "A wiki of articles arranged in a tree, with draft, published and archived states and tags. Visibility is only a label: there are no permissions per article yet, so everyone who can open the app reads every article.",
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
      "Employees file expense claims that move from draft to submitted, approved and paid. Paying needs its own permission and a person's approval. Receipts are mentioned in a note, not stored.",
  },
];
