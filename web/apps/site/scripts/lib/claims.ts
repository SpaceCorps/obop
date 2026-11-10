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
    why: "a person with the group acts directly; approval is asked of an AI assistant (crates/kernel/obop-commands/src/lib.rs:1095)",
    example:
      "A separate erase step, which a person must approve, removes a contact's personal details.",
  },
  {
    // Not "when an AI assistant proposes it": that is the true sentence.
    re: /\bneeds? approval\b(?!\s+(?:when|if|from) an AI assistant)/i,
    why: "approval is asked of an AI assistant only (crates/kernel/obop-commands/src/lib.rs:1095)",
    example: "Publishing needs approval.",
    fine: ["A refund needs approval when an AI assistant proposes it."],
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
    re: /never see sensitive details, such as/i,
    why: "money is in clear for an AI assistant wherever its class is none: Fleet cost, Purchase and POS totals, eCommerce order and payment amounts (packages/fleet.toml:192-195, purchase.toml:117-121, pos.toml, ecommerce.toml), and obop-masking allows that class (crates/kernel/obop-masking/src/lib.rs:15-33)",
    example:
      "AI assistants never see sensitive details, such as money amounts, bank details, pay, health data or sign-in details.",
    fine: ["AI assistants never see sensitive details. In this app: sensitive financial details."],
  },
  {
    re: /see only part of/i,
    why: "a masked field is shown as a placeholder, never in part (crates/kernel/obop-commands/src/lib.rs:1381-1387, crates/kernel/obop-masking/src/lib.rs:22-30)",
    example: "AI assistants see only part of people's personal details.",
    fine: ["AI assistants see names, emails and other personal details only as placeholders."],
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
      "Record that the goods arrived. If the order has a receiving location, a draft stock move is prepared for each line, for the warehouse to confirm, and you need permission to use Inventory.",
    ],
  },
  {
    re: /drafts a first assessment|keep ownership of the replies/i,
    why: "a ticket holds no assessment and no reply (packages/helpdesk.toml:57-165)",
    example:
      "A typical use is an AI assistant that reads new tickets and drafts a first assessment, while people keep ownership of the replies.",
  },
  {
    re: /\bAll 28 obop apps\b/i,
    why: "the marketplace bundles 24 packages, two of them (Accounting Sweden, DocuSeal) have no page here (crates/kernel/obop-pkg/src/marketplace.rs:153-177), and Mileage is built in (crates/obop/src/app.rs:77-84)",
    example: "All 28 obop apps: 6 that come with obop and 22 more you can add.",
    fine: ["Here are 28 obop apps: 6 that come with obop and 22 more you can add."],
  },
];

/** The same rows as pairs, for the tests that only need the pattern and the reason. */
export const OVERSTATED: readonly (readonly [RegExp, string])[] = CLAIMS.map((c) => [c.re, c.why]);
