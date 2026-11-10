/**
 * Sentences that were on an apps page, in a card, in the large text or in the technical part, and
 * that the module does not stand behind. Each carries the file that shows it is not so, as it was
 * when the sentence was found. A list of past mistakes keeps them from coming back and finds no new
 * one; the other half is the review of the manifest facts a card may assert, which a reviewer does.
 * Read by the unit tests (the pages as built from the data) and by the browser tests (the pages as
 * deployed), so the two cannot disagree.
 */
export const OVERSTATED: readonly [RegExp, string][] = [
  [/credit notes?/i, "no credit note exists (crates/modules/invoicing/src/lib.rs)"],
  [/to the whole list/i, "nothing sends a campaign (packages/email_marketing.toml)"],
  [
    /irreversible and reaches real people/i,
    "nothing sends an email (packages/email_marketing.toml)",
  ],
  [/nothing more is posted/i, "closing a year stops no posting (packages/accounting.toml)"],
  [/against further posting/i, "closing a period stops no posting (packages/accounting.toml)"],
  [/close each month and year/i, "closing stops no posting (packages/accounting.toml)"],
  [/closing[^.]*\bstops\b[^.]* entries/i, "closing stops no posting (packages/accounting.toml)"],
  [/send a draft order to the vendor/i, "the order is only marked sent (packages/purchase.toml)"],
  [/commit to the order with the vendor/i, "nothing reaches the vendor (packages/purchase.toml)"],
  [/start billing/i, "no subscription bills anything (packages/subscriptions.toml)"],
  [/return the money/i, "only a Stripe refund returns money (obop-payments/src/simple.rs)"],
  [/refund a paid sale to the customer/i, "the till refund is a record (packages/pos.toml)"],
  [/note that this was done/i, "the erase leaves no note (crates/modules/base/src/lib.rs)"],
  [
    /assign its legal number/i,
    "the module makes no legal claim (crates/modules/invoicing/src/lib.rs)",
  ],
  [/shared by all the other apps/i, "only 11 of the 27 other apps use a Contact (the manifests)"],
  [
    /which a person must approve/i,
    "a person with the group acts directly (obop-commands/src/lib.rs:1095)",
  ],
  [/needs? approval/i, "approval is asked of agents only (obop-commands/src/lib.rs:1095)"],
  [/only people who may approve/i, "said once, by the mark on the card"],
  [/stock at both places/i, "a receipt or a delivery has one place (packages/inventory.toml)"],
  [/receive customer support requests/i, "nothing receives them (packages/helpdesk.toml)"],
];
