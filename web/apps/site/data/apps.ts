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
      "Keep track of possible sales, from the first contact until you win or lose them. Each lead has a title, a contact, the revenue you expect and a stage. Moving a lead along the pipeline is a single action that your team and its AI agents can both take.",
  },
  {
    id: "sales",
    category: "customers",
    name: "Sales",
    tagline: "Quotations and orders",
    purpose:
      "Write quotations for customers and turn them into confirmed orders. Each order belongs to a contact and has lines, a total and a currency. Confirming an order prepares a draft invoice with the same lines, for Invoicing to post.",
  },
  {
    id: "invoicing",
    category: "finance",
    name: "Invoicing",
    tagline: "Bill your customers",
    purpose:
      "Prepare invoices for your customers as drafts, then post each one to give it its own number, in an unbroken yearly series. Posting an invoice always needs a person: an AI agent can prepare the draft and suggest posting it, and someone with the right permission approves it.",
    cards: {
      invoice: {
        text: "A customer invoice: its lines, a total that adds them up, and a number once it is posted.",
      },
    },
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
    cards: {
      "party.erase": { text: "Remove a contact's personal details and note that this was done." },
    },
  },
  {
    id: "connectors",
    category: "foundation",
    name: "Connectors",
    tagline: "Connect your other tools",
    purpose:
      "Connect your other tools to obop, with privacy controls built in. These are technical controls that support GDPR compliance, not legal advice or a certification. The catalogue says for every service how much of it is built. Nothing goes out or comes in until the reason for sharing is written down and the details that may be shared are listed. Personal details go out under a code name (a pseudonym) unless you decide otherwise, and sensitive details stay back. Every transfer is logged without its content. When someone withdraws their consent, the service is disconnected and its sign-in details are deleted. obop also keeps a list of what each connection processes, which you can export as a starting point for your own record of processing (GDPR Article 30).",
    technical:
      "Personal data leaves as keyed pseudonyms by default, the sensitive data classes stay withheld, every flow is recorded without its payload, and withdrawing consent disconnects the service and purges its credentials. The processing register is a generated read model over the connections, providers and consents, exportable as CSV; the complete Article 30 record stays with the operator.",
    modelNames: {
      connector_provider: "Service",
      provider_policy: "Service settings",
      egress_policy: "Data sharing settings",
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
      "Receive customer support requests and work through them, with priorities and response-time targets. A typical use is an AI agent that reads new tickets and drafts a first assessment, while people keep ownership of the replies.",
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
        text: "Remove an available vehicle from the fleet for good (only people who may approve).",
      },
    },
  },
  {
    id: "inventory",
    category: "operations",
    name: "Inventory",
    tagline: "Stock and warehouses",
    purpose:
      "Describe your warehouses and locations, record stock levels and log stock moves. When a stock manager confirms a move, the stock at both places is updated for you. Counting is still done by people.",
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
      "Keep your vendors and the purchase orders you send them, with order lines and a flow from draft to received. The order total is added up from its lines.",
    cards: {
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
      "A register of employees, departments and leave requests, with strict rules about who sees what. Pay, bank details and national id are visible to managers only, and health or union data only to a special group of readers. AI agents never see the sensitive kinds.",
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
      "Keep track of a shop till: open and close the cash drawer, and record each sale and how it was paid. It records what happened at the till. It is not a connection to card terminals, and card numbers must never be stored.",
    modelNames: {
      pos_config: "Point of sale",
    },
    cards: {
      "pos_order.pay": {
        text: "Mark the sale as fully paid. Not allowed until the payments cover the total. Each line also prepares a stock move, so you need permission to use Inventory.",
      },
    },
  },
  {
    id: "ecommerce",
    category: "operations",
    name: "eCommerce",
    tagline: "Online store",
    purpose:
      "Keep the data of an online shop: listings, shipping methods, discount codes and web orders. Orders follow a fulfilment flow, and refunds need a manager and an approval. When the public site is turned on, obop shows the shop to visitors and takes their orders.",
    technical:
      "With the public site turned on, obop shows the shop to visitors and takes orders; a custom storefront can use the API instead.",
  },
  {
    id: "website",
    category: "content",
    name: "Website",
    tagline: "Pages and forms",
    purpose:
      "Hold the content of a public website: pages, menus, blog posts and the forms visitors send. This app stores and organises the content; with the public site turned on, obop shows it to visitors. Publishing needs approval.",
  },
  {
    id: "email_marketing",
    category: "content",
    name: "Email Marketing",
    tagline: "Campaigns",
    purpose:
      "Keep mailing lists with each contact's consent status and date, and prepare campaigns. obop does not send the emails itself: marking a campaign as sent only records that it went out, and when an AI agent does it, a person has to approve it.",
    cards: {
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
      "Keep templates and signature requests, and follow each request from draft to signed. This is a simple electronic signature: the signer types their name, which does not prove who they are. For a PDF from Documents the app keeps a fingerprint and an exact copy. Only an administrator can change a signed request.",
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
      "Record what your business earns and spends, post each entry to the ledger, and close each month and year. Each entry has debit and credit lines, shows its totals and cannot be posted unless they match. Posted entries are numbered in an unbroken yearly series. Tax amounts are typed in by hand. Posting and closing need approval.",
    modelNames: {
      accounting_move: "Journal entry",
      accounting_move_line: "Journal entry line",
    },
    cards: {
      "accounting_fiscal_year.close": {
        text: "Close the fiscal year so nothing more is posted in it (only people who may approve).",
      },
      "accounting_fiscal_year.reopen": {
        text: "Reopen a closed fiscal year to correct it (only people who may approve).",
      },
      "accounting_period.close": {
        text: "Close the period against further posting (only people who may approve).",
      },
      "accounting_period.reopen": {
        text: "Reopen a closed period (only people who may approve).",
      },
      "accounting_move.post": {
        text: "Post a draft entry to the ledger and number it (only people who may approve; refused unless debits equal credits).",
      },
      "accounting_move.reverse": {
        text: "Mark a posted entry as reversed; the opposite entry is booked separately (only people who may approve).",
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
      "Employees file expense claims that move from draft to submitted, approved and paid. Paying needs its own permission and a person's approval. Receipts are mentioned in a note, not stored.",
  },
];
