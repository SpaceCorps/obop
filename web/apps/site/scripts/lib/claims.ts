/**
 * Sentences that were on an apps page, in a card, in the large text or in the technical part, and
 * that the module does not stand behind. A list of past mistakes keeps them from coming back and
 * finds no new one; the other half is the review of the manifest facts a card may assert, which a
 * reviewer does.
 *
 * Every row proves that it can fail: `example` is the sentence as it stood on a page (or in the
 * manifest, where the site shows it as it is), and the unit test asserts that `re` matches it, so a
 * pattern that is deleted, weakened or mistyped fails the test; `fine` are sentences that are true
 * and are on the pages now, which `re` must leave alone, so a pattern cannot grow so broad that it
 * catches the correction. `why` names the file that shows the sentence is not so, as it was when
 * it was found. Read by the unit tests (the pages as built from the data) and by the browser tests
 * (the pages as deployed), so the two cannot disagree. The count is pinned in both, so a row cannot
 * be dropped without saying so.
 */
export interface Claim {
  /** What the page said. */
  re: RegExp;
  /** Why it is not so. */
  why: string;
  /** A sentence the pattern must catch. */
  example: string;
  /** True sentences the pattern must not catch. */
  fine?: readonly string[];
}

export const CLAIMS: readonly Claim[] = [
  {
    re: /(?<!\bno )\bcredit notes?\b/i,
    why: "no credit note exists (crates/modules/invoicing/src/lib.rs:17)",
    example: "A customer invoice; posted invoices are corrected only by credit notes",
    fine: ["There are no credit notes in this app."],
  },
  {
    re: /to the whole list/i,
    why: "nothing sends a campaign (packages/email_marketing.toml:201)",
    example: "Send the scheduled campaign to the whole list (manager only)",
  },
  {
    re: /irreversible and reaches real people/i,
    why: "nothing sends an email (packages/email_marketing.toml:5)",
    example:
      "Sending is irreversible and reaches real people, so it needs a human approval from a manager",
  },
  {
    re: /nothing more is posted/i,
    why: "closing a year stops no posting (packages/accounting.toml:191, assert at :346 only)",
    example: "Close the fiscal year so nothing more is posted in it (only people who may approve).",
  },
  {
    re: /against further posting/i,
    why: "closing a period stops no posting (packages/accounting.toml:251)",
    example: "Close the period against further posting (only people who may approve).",
  },
  {
    re: /close each month and year/i,
    why: "closing stops no posting (packages/accounting.toml)",
    example: "post each entry to the ledger, and close each month and year.",
  },
  {
    // The claim, not one wording of it: "closing ... stops / blocks / locks ... posting / entries".
    // A verb in the third person, so "it does not stop new entries" is left alone.
    re: /\bclos(?:e|es|ing|ed)\b[^.;]*?(?<!\b(?:not|never) )\b(?:stops|blocks|prevents|locks|freezes|seals)\b[^.]*\b(?:post\w*|entr\w+)/i,
    why: "closing stops no posting (packages/accounting.toml: the only assert is line 346)",
    example: "Closing a month stops posting.",
    fine: [
      "Closing a month or year is only a label; it does not stop new entries.",
      "Mark the period as closed. This is a label only: entries can still be posted into it.",
    ],
  },
  {
    re: /send a draft order to the vendor/i,
    why: "the order is only marked sent (packages/purchase.toml:138-143)",
    example: "Send a draft order to the vendor as a request for quotation",
  },
  {
    re: /commit to the order with the vendor/i,
    why: "nothing reaches the vendor (packages/purchase.toml:145-152)",
    example: "Commit to the order with the vendor (manager only)",
  },
  {
    re: /start billing/i,
    why: "no subscription bills anything (packages/subscriptions.toml:136-141)",
    example: "Start billing at the end of the trial",
  },
  {
    re: /return the money/i,
    why: "only a Stripe refund returns money (crates/kernel/obop-payments/src/simple.rs)",
    example: "Return the money for a paid order that was not delivered (manager only)",
  },
  {
    re: /refund a paid sale to the customer/i,
    why: "the till refund is a record (packages/pos.toml:243-249)",
    example: "Refund a paid sale to the customer (manager only)",
  },
  {
    re: /note that this was done/i,
    why: "the erase writes no note on the contact (crates/modules/base/src/lib.rs:97-103); only the Connectors app logs an erasure, and only if it held something (crates/kernel/obop-connectors/src/egress.rs:643-693)",
    example: "Remove a contact's personal details and note that this was done.",
  },
  {
    re: /legal number/i,
    why: "the module gives a gapless yearly number and claims no legal status for it (crates/modules/invoicing/src/lib.rs:64, :69-70, :75)",
    example: "Gapless legal number such as INV-2026-00042, assigned when posted",
    fine: ["Gapless yearly number such as INV-2026-00042, assigned when the invoice is posted"],
  },
  {
    re: /shared by all the other apps/i,
    why: '8 of the other 27 apps refer to a Contact: sales and invoicing (crates/modules/*/src/lib.rs `.reference("party")`), accounting, ecommerce, field_service, pos, purchase and subscriptions (packages/*.toml `to = "party"`); the CRM lead keeps its contact as text (crates/modules/crm/src/lib.rs:26-39)',
    example: "One record for every person and company you deal with, shared by all the other apps.",
  },
  {
    re: /shared party record that CRM/i,
    why: "the CRM lead keeps contact_name and contact_email as text, not a reference (crates/modules/crm/src/lib.rs:26-39)",
    example:
      "People and organisations the business deals with: the shared party record that CRM, Sales and Invoicing refer to.",
    fine: [
      "People and organisations the business deals with: the party record that Sales, Invoicing, Purchase and other apps refer to.",
    ],
  },
  {
    re: /which a person must approve/i,
    why: "a person with the group acts directly; approval is asked of an AI agent (crates/kernel/obop-commands/src/lib.rs:1095)",
    example:
      "A separate erase step, which a person must approve, removes a contact's personal details.",
  },
  {
    // Not "when an AI agent proposes it" and not "when a person runs it": those are the true sentences.
    // "needs an approval" slipped past the first form of this row, which was written for "Publishing
    // needs approval."
    re: /\bneeds? (?:an? )?approval\b(?!\s+(?:(?:when|if|from) an AI agent|when a person runs it))/i,
    why: "approval is asked of an AI agent only (crates/kernel/obop-commands/src/lib.rs:1095)",
    example: "No step in this app needs an approval, apart from deleting records.",
    fine: [
      "A refund needs approval when an AI agent proposes it.",
      "A refund needs an approval when an AI agent proposes it.",
      "No step in this app needs approval when a person runs it.",
    ],
  },
  {
    re: /only people who may approve/i,
    why: "said once, by the mark on the card",
    example: "Remove an available vehicle from the fleet for good (only people who may approve).",
  },
  {
    re: /stock at both places/i,
    why: "a receipt or a delivery has one place (packages/inventory.toml:148-160)",
    example: "the stock at both places is updated for you.",
  },
  {
    re: /receive customer support requests/i,
    why: "nothing receives them (packages/helpdesk.toml)",
    example: "Receive customer support requests and work through them",
  },
  {
    re: /\bfinali[sz]\w*/i,
    why: "the product's word is post, and nothing locks a posted record: invoicing has no lock (crates/modules/invoicing/src/lib.rs:72-85) and accounting has one assert, the balance (packages/accounting.toml:346)",
    example: "then finalise each one to give it its own number",
    fine: ["Post a draft invoice and give it its next number in the yearly series."],
  },
  {
    re: /never see sensitive details/i,
    why: "money is in clear for an AI agent wherever its class is none: Fleet cost, Purchase and POS totals, eCommerce order and payment amounts (packages/fleet.toml:192-195, purchase.toml:117-121, pos.toml, ecommerce.toml), and obop-masking allows that class (crates/kernel/obop-masking/src/lib.rs:15-33)",
    example:
      "AI agents never see sensitive details, such as money amounts, bank details, pay, health data or sign-in details.",
    fine: [
      "AI agents never see the fields marked as sensitive. In this app: sensitive financial details. A price or a total is shown unless its field is one of these.",
    ],
  },
  {
    re: /see only part of/i,
    why: "a masked field is shown as a placeholder, never in part (crates/kernel/obop-commands/src/lib.rs:1381-1387, crates/kernel/obop-masking/src/lib.rs:22-30)",
    example: "AI agents see only part of people's personal details.",
    fine: [
      "AI agents see the fields marked as personal data, such as names and emails, only as placeholders.",
    ],
  },
  {
    re: /Copies in other apps are not touched/i,
    why: "the Connectors app deletes its code names and links for an erased contact (crates/modules/connectors/src/lib.rs:28, crates/kernel/obop-connectors/src/egress.rs:643-693)",
    example:
      "Replace a contact's name, email, tax number and notes with [erased], and remove the photo. Copies in other apps are not touched.",
  },
  {
    re: /only ever added to|append-only/i,
    why: "the purge deletes flow-log rows past their retention (crates/kernel/obop-connectors/src/runtime.rs:1455-1493)",
    example: "A record of every transfer that is only ever added to",
    fine: ["Entries older than the retention period are removed by the purge."],
  },
  {
    // A bare "obop does not <do the thing>" is wider than the module: a connection can post the record
    // to an endpoint on these commands, and sign_docuseal sends the request to DocuSeal on send.
    re: /(?<!\bBy itself, )(?<!\bOn its own, )\bobop does not (?:contact the vendor|email the signer|start any billing|pay money back|send it)\b/i,
    why: "a webhook connection can post the record on these commands (crates/kernel/obop-connectors/src/catalog.rs:222-262), and sign_docuseal sends the request on sign_request.send (packages/sign_docuseal.toml:5)",
    example: "Mark the order as sent to the vendor. obop does not contact the vendor.",
    fine: [
      "Mark the order as sent to the vendor. By itself, this does not contact the vendor.",
      "obop does not send the emails itself.",
    ],
  },
  {
    re: /(?:then|and) the employee confirms it|confirmed by the employee/i,
    why: "confirm has no group of its own, so it is for planning.manager (packages/planning.toml:149-155, crates/kernel/obop-pkg/src/manifest.rs:1793)",
    example: "it goes from open to published, and then the employee confirms it.",
    fine: [
      "Record that the employee confirmed they will work the shift. Only a planning manager can do this.",
    ],
  },
  {
    re: /Publish the shift to the assigned employee/i,
    why: "publish only changes the state: no effect, no notification (packages/planning.toml:133-139)",
    example: "Publish the shift to the assigned employee, or to everyone when it is open",
  },
  {
    re: /from reusable templates/i,
    why: "nothing makes a shift from a template: nothing in crates/ or web/apps/backoffice/src refers to planning_template",
    example: "Plan shifts for your employees from reusable templates.",
  },
  {
    re: /A draft stock move is prepared for each line, for the warehouse to confirm\. Needs permission/i,
    why: "the stock moves are made only if the order has a receiving location (packages/purchase.toml:160-165)",
    example:
      "Record that the goods arrived. A draft stock move is prepared for each line, for the warehouse to confirm. Needs permission to use Inventory.",
    fine: [
      "Record that the goods arrived. You need permission to use Inventory. If the order has a receiving location, a draft stock move is prepared for each line, for the warehouse to confirm.",
    ],
  },
  {
    re: /drafts a first assessment|keep ownership of the replies/i,
    why: "a ticket holds no assessment and no reply (packages/helpdesk.toml:57-165)",
    example:
      "A typical use is an AI agent that reads new tickets and drafts a first assessment, while people keep ownership of the replies.",
  },
  {
    re: /\bAll 28 obop apps\b/i,
    why: "the marketplace bundles 24 packages, two of them (Accounting Sweden, DocuSeal) have no page here (crates/kernel/obop-pkg/src/marketplace.rs:153-177), and Mileage is built in (crates/obop/src/app.rs:77-84)",
    example: "All 28 obop apps: 6 that come with obop and 22 more you can add.",
    fine: ["Here are 28 of the obop apps: 6 that come with obop and 22 more you can add."],
  },
  {
    re: /see names, emails and other personal details only as placeholders/i,
    why: "masking acts on the fields marked as personal data, and nothing reads free text: a name typed into a description reaches an AI agent as written (crates/kernel/obop-commands/src/lib.rs:1377-1378, crates/kernel/obop-masking/src/lib.rs:15-33)",
    example: "AI agents see names, emails and other personal details only as placeholders.",
    fine: [
      "AI agents see the fields marked as personal data, such as names and emails, only as placeholders. Text typed into a field that is not marked as personal data, such as a description, is not replaced by a placeholder.",
    ],
  },
  {
    re: /for the warehouse to confirm, and you need permission/i,
    why: "the permission is needed whether or not the order has a receiving location: the guard looks at the effect and not at its condition (crates/kernel/obop-commands/src/guards.rs:244-258, the condition is at packages/purchase.toml:160-165)",
    example:
      "Record that the goods arrived. If the order has a receiving location, a draft stock move is prepared for each line, for the warehouse to confirm, and you need permission to use Inventory.",
    fine: [
      "Record that the goods arrived. You need permission to use Inventory. If the order has a receiving location, a draft stock move is prepared for each line, for the warehouse to confirm.",
    ],
  },
  {
    re: /\bNothing goes out or comes in until\b/i,
    why: "the Google Drive connection is connected by the OAuth callback with no consent record (crates/kernel/obop-connectors/src/runtime.rs:1175-1209, drive.rs:320-372); the check for one is only on connection.connect (crates/modules/connectors/src/handlers.rs:298)",
    example: "Nothing goes out or comes in until the reason for sharing is written down.",
    fine: [
      "No record goes out or comes in through a connection until the reason for sharing is written down.",
    ],
  },
  {
    re: /the money is returned too/i,
    why: "obop asks Stripe to refund and accepts a pending refund (crates/kernel/obop-payments/src/stripe.rs:247-262); it was tested against a stand-in only (stripe.rs:15-16)",
    example:
      "For card payments through Stripe the money is returned too; otherwise you pay it back yourself.",
    fine: [
      "For card payments through Stripe, obop asks Stripe to refund the payment first, and the order stays paid if Stripe refuses. Otherwise you pay it back yourself.",
    ],
  },
  {
    re: /closed to new postings|can still receive entries|can still be chosen on new lines/i,
    why: "nothing reads these flags, and the only assert of the package is the balance (packages/accounting.toml:346)",
    example: "Whether the account is closed to new postings",
    fine: ["A label only: entries can still be posted to a deprecated account."],
  },
  {
    re: /customer (?:must|has to) sign off/i,
    why: "completing a job has no assert and no effect, and nothing reads signature_required (packages/field_service.toml:168-175)",
    example: "which parts were used and whether the customer must sign off.",
    fine: ["which parts were used and a note of whether the customer is to sign off."],
  },
  {
    re: /Whether visitors can subscribe themselves/i,
    why: "the package has no public section and nothing reads is_public (packages/email_marketing.toml)",
    example: "Whether visitors can subscribe themselves",
    fine: ["A label only; nothing checks it, and this app has no sign-up page for visitors."],
  },
  {
    // The same shape as the row above for "obop does not ...": a flat denial of sending.
    re: /(?<!\bBy itself, )(?<!\bOn its own, )\bobop sends no\b/i,
    why: "a webhook connection can post the record on any command (crates/kernel/obop-connectors/src/catalog.rs:222-262), and sign_docuseal sends the request on sign_request.send (packages/sign_docuseal.toml:5)",
    example:
      "Email address of the signer; optional, obop sends no email for a request about a document",
    fine: [
      "Email address of the signer; optional. By itself, obop sends no email for a request about a document.",
    ],
  },
  {
    re: /draft document the current version/i,
    why: "publish only moves the state to published; the model has no version or history field (packages/documents.toml:130-135)",
    example: "Make a draft document the current version",
    fine: ["Mark a draft document as published."],
  },
  {
    re: /Whether (?:the recipe can be used for new orders|the resource can be booked|new customers can pick the plan|the policy can be attached to new tickets|sessions must count the cash|cashiers may propose refunds)|empty for a private task/i,
    why: "nothing reads these flags (the asserts of all packages are accounting.toml:346, pos.toml:226 and sign.toml:202), and projects_task has no [model.access], so a task without a project is not private",
    example: "Whether the resource can be booked",
    fine: ["Marks the resource as bookable or not. A label only; nothing checks it."],
  },
  {
    re: /Each order belongs to a contact/i,
    why: "party_id of a sale order is not required (crates/modules/sales/src/lib.rs:29-36)",
    example: "Each order belongs to a contact and has lines, a total and a currency.",
    fine: ["Each order can name a contact and has lines, a total and a currency."],
  },
  {
    re: /read, search, create and update records of this app/i,
    why: "some models are read-only (the logs, runs and catalogue views of Connectors: crates/modules/connectors/src/handlers.rs:532-542) and sign_link.create is refused to anything but a person (crates/kernel/obop-commands/src/sign/link.rs:48-55)",
    example:
      "An agent can read, search, create and update records of this app, and run the actions that need no approval.",
    fine: [
      "An agent can read and search records of this app, create and update them where the model allows it, and run the actions that need no approval.",
    ],
  },
  {
    // Wordings of earlier rounds that no test would have caught if they were put back.
    re: /\bsends? the order to the vendor\b/i,
    why: "the order is only marked sent; nothing reaches the vendor (packages/purchase.toml:138-143)",
    example: "Purchase sends the order to the vendor.",
    fine: ["Mark the order as sent to the vendor. By itself, this does not contact the vendor."],
  },
  {
    re: /AI agents never see (?:money|amounts?|financial)/i,
    why: "money is in clear for an AI agent wherever its class is none (crates/kernel/obop-masking/src/lib.rs:15-33)",
    example: "AI agents never see money amounts.",
    fine: [
      "AI agents never see the fields marked as sensitive. In this app: sensitive financial details.",
    ],
  },
  {
    re: /leaves? other apps alone|other apps (?:are|stay|remain) (?:alone|untouched|unchanged)/i,
    why: "the Connectors app deletes its code names for an erased contact, and the copies in other records stay (crates/kernel/obop-connectors/src/egress.rs:643-693)",
    example: "Erasing a contact leaves other apps alone.",
    fine: [
      "Names, emails and addresses typed into other records (such as leads, orders and tickets) are not changed.",
    ],
  },
  {
    // Round 7: the wordings that the last sweep took out of the pages.
    re: /\bPersonal details go out under a code name\b|\bsensitive details stay back\b/i,
    why: "only a field marked as personal data gets a code name, any other listed field goes out as written, and an impact assessment releases a sensitive kind (crates/kernel/obop-connectors/src/policy.rs:3-6, :66-73, :86)",
    example:
      "Personal details go out under a code name (a pseudonym) unless you decide otherwise, and sensitive details stay back.",
    fine: [
      "Fields marked as personal data go out under a code name (a pseudonym) unless you decide otherwise.",
      "Sensitive fields stay back by default; a person can release a sensitive kind for a connection only by writing down an impact assessment, and secrets never leave.",
    ],
  },
  {
    re: /Check the settings and the address without sending personal data/i,
    why: "for a service that receives records the test sends one empty event to the address (crates/kernel/obop-connectors/src/runtime.rs:1279-1313)",
    example: "Check the settings and the address without sending personal data",
    fine: [
      "Check the settings and the address. For some services that receive records from obop, it also sends one empty test event (no records, no personal data) to that address.",
    ],
  },
  {
    re: /\bWhen someone withdraws their consent\b|sign-in details[^.]*\b(?:are|is) deleted\b/i,
    why: "only the connection's owner or an administrator can withdraw (crates/modules/connectors/src/guards.rs:86-103, :668), and revoking clears only obop's own copy: the key or authorisation at the other service stays valid (crates/kernel/obop-connectors/src/runtime.rs:1351-1360)",
    example:
      "End the connection for good. Access stops, and the sign-in details and code-name lists are deleted.",
    fine: [
      "End the connection for good. obop stops using it and deletes its own copy of the sign-in details and code-name lists.",
      "Withdraw the reason for sharing (the connection's owner or an administrator): the connection ends and obop deletes its copy of the sign-in details.",
    ],
  },
  {
    re: /\bthe platform has no row-level permissions\b/i,
    why: "the platform has row-level rules (crates/kernel/obop-commands/src/engine/access.rs:1-8, used by packages/hr.toml, timesheets.toml, documents.toml, dashboards.toml); Knowledge declares none",
    example:
      "Visibility is a label for the UI and for people to respect; the platform has no row-level permissions yet, so everyone with the Knowledge groups can read every article.",
    fine: [
      "Visibility is a label for the UI and for people to respect; this package sets no row rules, so everyone with the Knowledge groups can read every article.",
    ],
  },
  {
    re: /\bPublishing (?:is for managers|makes content public and needs a human approval)\b/i,
    why: "the published flag of a translation and the visible flag of a menu entry are plain fields that publish with no approval (packages/website.toml:393-398, :145-149; crates/kernel/obop-commands/src/public.rs:259-263)",
    example:
      "Publishing is for managers; an AI agent can only propose it. Edits to a published page need no further approval.",
    fine: ["Publishing a page or a blog post is for managers; an AI agent can only propose it."],
  },
  {
    re: /reverse only with an HR decision/i,
    why: "no action leaves offboarded, so nothing reverses it (packages/hr.toml:199-213, crates/kernel/obop-commands/src/guards.rs:158)",
    example: "End the employment record; reverse only with an HR decision",
    fine: [
      "Mark an active employee as offboarded. The record and its details stay, and no step moves it back.",
    ],
  },
  {
    re: /Submit the request to the manager/i,
    why: "request only moves the state; nothing notifies a manager (packages/hr.toml:302-308)",
    example: "Submit the request to the manager",
    fine: [
      "Mark the draft as requested, for a manager to approve or refuse. By itself, this does not notify anyone.",
    ],
  },
  {
    re: /so it can be billed/i,
    why: "approving an entry only moves the state; nothing bills it and nothing reads timesheets_entry (packages/timesheets.toml:85-98)",
    example: "Approve a submitted entry so it can be billed (manager only)",
    fine: ["Mark a submitted entry as approved (manager only). By itself, this does not bill it."],
  },
  {
    re: /a done move cannot be undone/i,
    why: "no action reverses a done move, but its record is written by inventory.user and can be edited or deleted (packages/inventory.toml:126-129, :181-189)",
    example:
      "the quantity leaves the source item and arrives at the destination item (stock managers only); a done move cannot be undone",
    fine: [
      "No action reverses a done move, but its record can still be edited or deleted, and the stock does not follow.",
    ],
  },
  {
    re: /card numbers must never be stored/i,
    why: "nothing checks for a card number: provider_reference is a plain text field (packages/pos.toml:340-345)",
    example: "It is not connected to card terminals, and card numbers must never be stored.",
    fine: ["Do not type card numbers into any field: obop does not check for them."],
  },
  {
    re: /Confidentiality is enforced/i,
    why: "the instance administrator holds every group and sees every record (crates/kernel/obop-commands/src/engine/access.rs:1-8, :57-59; crates/kernel/obop-commands/src/lib.rs:105-118)",
    example:
      "Confidentiality is enforced: confidential and restricted documents are visible only to their owner and to documents.manager",
    fine: [
      "Confidentiality limits who can see a document: confidential and restricted documents are visible only to their owner, to documents.manager and to the instance administrator",
    ],
  },
  {
    re: /\bposted only when the totals match\b/i,
    why: "the totals are checked once, when post runs; the lines of a posted entry stay editable and an entry with no lines posts (packages/accounting.toml:346, :368)",
    example: "An entry is a set of debit and credit lines, posted only when the totals match.",
    fine: ["An entry is a set of debit and credit lines; its totals are checked only at posting."],
  },
];

/** The same rows as pairs, for the tests that only need the pattern and the reason. */
export const OVERSTATED: readonly (readonly [RegExp, string])[] = CLAIMS.map((c) => [c.re, c.why]);
