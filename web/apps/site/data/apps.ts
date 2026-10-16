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
  purpose: string;
}

export const BUILTIN_IDS = ["base", "product", "crm", "sales", "invoicing"] as const;

export const APP_COPY: AppCopy[] = [
  {
    id: "crm",
    category: "customers",
    name: "CRM",
    tagline: "Leads and pipeline",
    purpose:
      "Keep track of prospective deals from first contact to won or lost. A lead has a title, a contact, an expected revenue and a stage, and moving it along the pipeline is a named action that the team and its agents can both run.",
  },
  {
    id: "sales",
    category: "customers",
    name: "Sales",
    tagline: "Quotations and orders",
    purpose:
      "Write quotations for customers and turn them into confirmed orders. Orders point at a contact, carry a total and a currency, and confirmed orders are what Invoicing bills.",
  },
  {
    id: "invoicing",
    category: "finance",
    name: "Invoicing",
    tagline: "Drafts, posting, numbering",
    purpose:
      "Prepare customer invoices as drafts and post them to give each one its gapless legal number. Posting is the one step in the built-in apps that always needs a person: an agent can prepare the draft and propose the posting, and a human with the right permission approves it.",
  },
  {
    id: "product",
    category: "foundation",
    name: "Products",
    tagline: "Prices, SKUs, cost",
    purpose:
      "The catalogue of what you sell: name, SKU, list price and unit cost. Cost is visible only to people in the cost viewer group, so a sales assistant or an agent can quote without seeing margins.",
  },
  {
    id: "base",
    category: "foundation",
    name: "Contacts",
    tagline: "People and companies",
    purpose:
      "One record for every person and organisation you deal with, shared by all other apps. Names and emails are personal data and are masked for agents; the tax identifier is withheld from them entirely. A dedicated, approval-gated erase action anonymises a contact in place.",
  },
  {
    id: "projects",
    category: "people",
    name: "Project",
    tagline: "Projects and tasks",
    purpose:
      "Plan work as projects, milestones and tasks with assignees and deadlines. Progress is tracked by status, which keeps the model simple; it is a good fit for small teams that want a shared board rather than effort accounting.",
  },
  {
    id: "helpdesk",
    category: "customers",
    name: "Helpdesk",
    tagline: "Support tickets",
    purpose:
      "Receive and work through customer support requests with priority, severity and service-level targets. A typical use is an agent that reads new tickets and drafts a first triage while people keep ownership of the replies.",
  },
  {
    id: "fleet",
    category: "operations",
    name: "Fleet",
    tagline: "Vehicles and drivers",
    purpose:
      "Keep a register of company vehicles, who drives them and their service history. It records status and services; it does not compute running costs.",
  },
  {
    id: "inventory",
    category: "operations",
    name: "Inventory",
    tagline: "Stock and warehouses",
    purpose:
      "Describe warehouses and locations, record stock levels and log stock moves. Quantities are entered by people, so it works as a ledger of what was counted and moved rather than an automatic stock engine.",
  },
  {
    id: "purchase",
    category: "operations",
    name: "Purchase",
    tagline: "Vendors and purchase orders",
    purpose:
      "Keep vendors and the purchase orders you send them, with order lines and a draft to received flow. Totals are entered, not summed from the lines.",
  },
  {
    id: "manufacturing",
    category: "operations",
    name: "Manufacturing",
    tagline: "Bills of materials",
    purpose:
      "Define what a product is made of and plan manufacturing orders through draft, confirmed, in progress and done. Component stock is not consumed automatically.",
  },
  {
    id: "hr",
    category: "people",
    name: "HR",
    tagline: "Employees and leave",
    purpose:
      "A register of employees, departments and leave requests, with strict data classes: pay, bank details and national id are visible to managers only and health or union data only to a special reader group. Agents never see the sensitive classes.",
  },
  {
    id: "timesheets",
    category: "people",
    name: "Timesheets",
    tagline: "Time and billing",
    purpose:
      "Record time spent on project tasks in minutes, mark it billable and run it through an approval step. Needs the Projects package.",
  },
  {
    id: "planning",
    category: "people",
    name: "Planning",
    tagline: "Shifts and resources",
    purpose:
      "Plan shifts for employees from reusable templates. A shift with nobody assigned is an open shift; the workflow runs from open through published to confirmed by the employee.",
  },
  {
    id: "field_service",
    category: "operations",
    name: "Field Service",
    tagline: "Jobs on site",
    purpose:
      "Schedule on-site jobs for customers: who does the work, where, when, which parts were used and whether the customer must sign off. Site addresses and contacts are personal data and treated as such.",
  },
  {
    id: "pos",
    category: "operations",
    name: "Point of Sale",
    tagline: "Retail and restaurants",
    purpose:
      "Model a shop till: configuration, cash-controlled sessions, orders with lines and payment lines per method. It records what happened at the till; it is not a payment terminal integration, and card numbers must never be stored.",
  },
  {
    id: "ecommerce",
    category: "operations",
    name: "eCommerce",
    tagline: "Online store",
    purpose:
      "Store the data of an online shop: listings, shipping methods, discount codes and web orders with a fulfilment flow. Refunds need a manager and an approval. A storefront or other client reads and writes this data through the API.",
  },
  {
    id: "website",
    category: "content",
    name: "Website",
    tagline: "Pages and forms",
    purpose:
      "Hold the content of a public website: pages, menus, blog posts and the form submissions visitors send. This app stores and organises content; something else renders it. Publishing needs approval.",
  },
  {
    id: "email_marketing",
    category: "content",
    name: "Email Marketing",
    tagline: "Campaigns",
    purpose:
      "Keep mailing lists with each contact's consent status and date, and prepare campaigns. Sending reaches real people and cannot be undone, so it always needs a human approval.",
  },
  {
    id: "documents",
    category: "content",
    name: "Documents",
    tagline: "Files and folders",
    purpose:
      "A register of business documents with folder, kind, owner, expiry and confidentiality. It stores references to documents, not the file contents.",
  },
  {
    id: "sign",
    category: "content",
    name: "Sign",
    tagline: "E-signatures",
    purpose:
      "Manage templates and signature requests through draft, sent, signed, declined and expired, and record the document hash that was signed. The platform cannot yet lock a record, so a signed request is immutable by convention only.",
  },
  {
    id: "subscriptions",
    category: "customers",
    name: "Subscriptions",
    tagline: "Recurring billing",
    purpose:
      "Define recurring plans and track customer subscriptions with trial, active, past due and cancelled states, including the next invoice date. It does not create invoices by itself.",
  },
  {
    id: "dashboards",
    category: "content",
    name: "Dashboards",
    tagline: "Reports and charts",
    purpose:
      "Store dashboards and widgets as data (KPI, bar, line, pie and table definitions) for a client to render. The package never runs a query and does not check that the models it names exist.",
  },
  {
    id: "accounting",
    category: "finance",
    name: "Accounting",
    tagline: "Ledger and reports",
    purpose:
      "A chart of accounts, journals, tax rates, journal entries with debit and credit lines, fiscal periods and bank statement lines. Nothing is computed for you: balancing and tax amounts are the responsibility of whoever enters them. Posting and closing need approval.",
  },
  {
    id: "knowledge",
    category: "content",
    name: "Knowledge",
    tagline: "Articles and guides",
    purpose:
      "A wiki of articles arranged in a tree, with draft, published and archived states and tags. Visibility is a label: there are no row-level permissions yet, so everyone with access to the app reads every article.",
  },
  {
    id: "appointments",
    category: "customers",
    name: "Appointments",
    tagline: "Bookable resources",
    purpose:
      "Describe bookable people, rooms and equipment and record appointments with a date, start and end. Overlaps are not prevented automatically.",
  },
  {
    id: "expenses",
    category: "finance",
    name: "Expenses",
    tagline: "Claims and approvals",
    purpose:
      "Employees file expense claims that move through draft, submitted, approved and paid. Paying needs its own permission and a human approval; receipts are referenced by note, not stored.",
  },
];
