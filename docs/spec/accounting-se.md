# Accounting Sweden: moving open-accounting-sweden into obop

- **Status:** Draft 0.1. PROPOSED. No owner ruling covers this area yet.
- **Date:** 2026-11-03
- **Scope:** How the domain model of open-accounting-sweden (OAS, our own project, MIT) maps onto obop, in stages: the `accounting_se` package, SIE 4 files, an importer for OAS data, and the later pieces (invoices, payroll, receipts, time).
- **Not verified:** Nothing in this document or in the package has been checked against Skatteverket (form SKV 4700, the AGI specification, any rate or deadline) or against the official SIE specification. Every box, code, rate and file layout named here comes from OAS, which itself marked them UNVERIFIED, or from general knowledge. An owner must check each one before any of it is relied on for a real return.

**Reading notes.** Decisions are cited by ID ([DECISIONS.md](../DECISIONS.md)). The package format is described in [MODULES.md](../MODULES.md). The ledger package this builds on is `packages/accounting.toml`. OAS is the repository `SpaceCorps/open-accounting-sweden`; "OAS" in this document means its server crates, `oas-server` (`vat.rs`, `ledger.rs`, `invoices.rs`, `payroll.rs`, `inbox.rs`) and its SQLite schema.

## 1. Purpose and scope

OAS is a small, self-hosted bookkeeping server for Swedish aktiebolag. obop has a general ledger package (`accounting`) and no Swedish rules. The plan is to carry the Swedish parts over as data models and importers on top of obop, not to port OAS code: obop's own commands, permissions, approvals and surfaces replace the OAS server, and OAS stays what it is.

Two rules shape every stage.

1. **No BAS chart in the public tree.** The BAS chart of accounts (kontoplan) and its account names are proprietary. Decision D34 and the provenance check in CI reject restricted data in the public tree. The package therefore contains no account number or account name list. Where a VAT code needs an account, the field is a reference the user fills from their own chart. A later step takes a BAS file that the licensee supplies (section 8).
2. **Everything starts unverified.** OAS marks every VAT box mapping UNVERIFIED until the owner checks it. The package keeps that: each box and each code has a `review` state that starts at `unverified`, and only the stricter `accounting.approver` group can move it.

## 2. Status of each stage

| Stage | What it is | Status |
|---|---|---|
| 1. Ledger | Accounts, journals, entries, periods, fiscal years, gapless numbering, approval to post | Built (`packages/accounting.toml`), plus the Swedish company and voucher series models of this PR |
| 2. VAT | VAT boxes, VAT codes, the VAT report per period with propose and approve | `packages/accounting_se.toml` (bundled in the catalogue; labels in de, fr, pl, fi, sv and es are machine-drafted); the boxes are computed from the ledger by `vat_computation.compute` (section 3.2); UNVERIFIED, never compared with Skatteverket or run on PostgreSQL |
| 3. SIE 4 | Import and export of SIE 4 files | Built: the crate `obop-sie` (file format) and the commands `obop sie export|import` and `sie_exchange.export|import` (section 4.1); tested on generated files only, NOT checked against the official SIE specification, never run against Fortnox, Visma or a real file |
| 4. OAS importer | `obop import oas` reads the OAS SQLite file and documents directory | Built for company, accounts, vouchers with entries, customers and receipts (`crates/obop/src/import_oas/`, section 5.3); invoices, payroll, bank, time are listed as skipped; tested against generated data only, never against real books |
| 5. Invoices | Customer invoices with the ledger voucher | First slice: `invoice_posting.create_move` books a posted invoice as a draft move (section 6); UNVERIFIED against Swedish invoice and VAT rules; credit notes, EU and export cases, payment matching not built |
| 6. Payroll and AGI | Employees, pay runs, employer contributions, AGI file | Data model only, in `packages/contrib/payroll_se/payroll_se.toml` ([payroll-se.md](payroll-se.md); installable by upload, not bundled; every rate and rule UNVERIFIED, nothing seeded); no computation, no AGI XML |
| 7. Receipts and bank | Receipt archive, bank lines, matching | Parts exist (`documents`, `accounting_bank_statement_line`); matching and booking not built |
| 8. Time tracking | Projects, time entries, billing | Parts exist (`timesheets` package); the link to invoices is not built |
| 9. Immutability and hash chain | Vouchers that cannot change, with a verifiable chain | Not built; waits on K5 (see 9) |

## 3. Stage 1 and 2: the `accounting_se` package

The package `accounting_se` depends on `accounting` (`^0.3`) and needs core 0.2.1 (it uses field limits, D90). It is declarative (X1) and ships no rows.

| Model | Holds | OAS source |
|---|---|---|
| `accounting_se_company` | Name, organisation number, VAT number, F-tax, fiscal year start month, VAT period (monthly, quarterly, yearly), currency, address, contact | table `company` |
| `accounting_se_voucher_series` | Series code (one to three letters, `A` in OAS), name, journal, default flag | column `voucher.series` |
| `accounting_se_vat_box` | Box number, title, kind (base, output, input, result), not-used flag, review state | table `vat_box` |
| `accounting_se_vat_code` | Code, treatment, rate in basis points, base box and side, VAT box, VAT account, VAT side, tax, review state | tables `vat_code` and `vat_account` |
| `accounting_se_vat_report` | One period: dates, due date, computed output VAT, input VAT and VAT to pay, the confirmation flag, state | the report built in `vat.rs` |
| `accounting_se_vat_report_line` | One box of a report with its amount and the voucher references behind it | the `boxes` array of the OAS report |

**Review state.** `review` is a select (`unverified`, `verified`) that the actions `verify` and `unverify` move; both need `accounting.approver` and a human approval, so an agent can only propose them. `verified` is a computed bool on the same record. Nothing in the package is created as verified.

**Report states.** `draft`, `proposed`, `approved`, `filed`. `propose` can be run by anyone with `accounting.user`, agents included. `approve` and `record_filed` need `accounting.approver` and an approval. `approve` is refused unless `boxes_confirmed` is true, the approver's statement that every box was compared with SKV 4700 and the mappings used are verified. `record_filed` only records that the return was filed somewhere else; nothing is sent to Skatteverket from obop.

**Amounts.** Money fields in minor units (öre). Rates in basis points (2500 is 25.00 percent). No floats. Output VAT, input VAT and VAT to pay on the report are computed from the lines (`sum(lines.amount where kind == "output")`), and VAT to pay is output minus input.

**Starter data (UNVERIFIED).** The package has no seed mechanism, so the rows below are a list to enter or import, taken from OAS and not checked against SKV 4700:

- Boxes: 05 (base), 10, 11, 12 (output VAT at 25, 12 and 6 percent), 21 and 22 (base, reverse-charge purchases of services from the EU and from outside it), 30, 31, 32 (output VAT on reverse charge at 25, 12 and 6 percent), 39 (base, sale of services to another EU country), 48 (input VAT), 49 (result).
- Codes: S25, S12, S6 (sale in Sweden), RCEU and RCNONEU (reverse-charge purchase), SEUSVC (sale of services to the EU), I25 (domestic purchase with input VAT), NONE. The treatment list also names export and exempt; OAS has no code for them yet.
- Which account feeds which box is not listed here on purpose (rule 1 of section 1).

**What X1 cannot express.** These are stated limits, not gaps in effort.

- **No derivation from the ledger.** A report cannot sum the ledger lines that carry a VAT code, because X1 has no joins, no aggregate over another model and no function that fills a record from several others. The preparer (a person, an agent or a later importer or X2 command) writes the report lines. OAS computes the boxes in SQL; that part is not ported.
- **No seeding.** A package cannot ship rows, so boxes and codes are entered or imported after install.
- **No VAT code on a ledger line.** A package cannot add a field to `accounting_move_line`, so a line cannot carry a `vat_code` the way an OAS entry does. The existing `tax_id` on the line stays the link; the mapping from code to tax is the optional `tax_id` of the code. Changing this needs a field in `accounting` itself (not done here).
- **Voucher series do not number.** The number of an entry is the gapless `JE/{year}/{seq}` series of `accounting`. A series record names a letter and a journal but cannot change the number format or keep its own counter. OAS numbers `A1`, `A2`, and so on; an import keeps the old number as a reference (section 5), not as the entry number.
- **Rows are not locked.** A report line or a box can be edited by anyone with the write group after a report is approved, because X1 does not lock records (MODULES.md section 10). The approve action stamps the state only.
- **Due date.** The date is entered. OAS computes it from the quarter and moves it off weekends; the rule has not been checked, so it is not copied.
- **Readiness checks.** OAS reports checks (unverified boxes, an amount in a box marked not used, reverse-charge consistency, vouchers without a code or receipt). `approve` has one assertion only (`boxes_confirmed`). The rest would need a command, not a model.

### 3.2 Computing a VAT report from the ledger (`vat_computation.compute`)

A typed command (REST, MCP, same functions as in `crates/obop/src/vat.rs`; group `accounting.user`) with input `report` (id or name) and `dry_run`. It reads the user's own `accounting_se_vat_box` and `accounting_se_vat_code` rows and the posted ledger; no box, code, rate or account is built in (D34), and nothing is checked against Skatteverket.

- Moves in state `posted` or `reversed` dated from `start_on` to `end_on` count; drafts and cancelled moves do not.
- A code maps a ledger line to a box through the `tax_id` it shares with the line: a line with that tax on any account but the code's VAT account adds to `base_box_id`; a line on `vat_account_id` adds to `vat_box_id`. A line on a VAT account without a tax is given to the one code whose tax is on another line of the same move; otherwise the run is refused and names the voucher. Each side is `credit - debit` or `debit - credit` as `base_side` and `vat_side` say. Amounts are integer ore, with overflow checks.
- It writes one `accounting_se_vat_report_line` per box any code points to (zero amounts included): kind copied from the box, amount, and the numbers of the vouchers behind it, one per line. The report totals (`output_vat`, `input_vat`, `vat_to_pay`) are the report's own computed fields over those lines. Result boxes get no line.
- It runs only on a report in state `draft`, replaces that report's lines, and clears `boxes_confirmed`. The report stays a draft; proposing and approving stay the person's, and `approve` still needs the approver's confirmation. An approved, proposed or filed report is refused.
- Refused in words: no code maps to a box, a code that maps to a box without `tax_id`, side or VAT account, two codes on one tax, a box that does not exist, an amount in a box marked `not_used`, an unattributable VAT line.
- `dry_run` computes and returns the lines and writes nothing. An agent may only dry-run; a real run needs a person. A run is not atomic across the delete and the create (a failure leaves the draft without lines; run it again).
- Tests: `crates/obop/src/vat_tests.rs` (invented chart on memory storage only). Parity gap: no CLI verb and no UI button (docs/ISSUES.md K59).

## 4. Stage 3: SIE 4 (crate `obop-sie`)

The crate reads and writes SIE 4 files (the Swedish accounting exchange format) for the `accounting` models. OAS already exports SIE 4 from `ledger.rs`.

- The export is a function of posted entries, accounts, fiscal years and the company record. It needs the account names, which come from the user's own chart.
- The import is the other half of the BAS problem: a SIE file carries account names that come from the sender's chart. Importing creates the accounts of that file in the user's database. Whether the public repository may hold a fixture file with BAS names is a D34 question, so fixtures use invented numbers and names.
- Status of correctness: the file layout used by OAS and by the crate has **not** been compared with the official SIE specification. Interoperability with Fortnox, Visma or a real auditor's tool is untested.

### 4.1 The commands (built)

`obop sie export --year YYYY [--out FILE] [--db URL] [--token T]`
- Builds a `SieFile` of the posted moves (states `posted` and `reversed`; a reversed move was posted and its opposite move is an ordinary posted one) whose date is in the year: the `accounting_fiscal_year` that starts in YYYY, else the calendar year. Writes CP437 bytes with CRLF to `--out` (default `YYYY.se`) and prints counts and the SHA-256. Needs `accounting.user`.
- Company name and organisation number come from `accounting_se_company` when that package is installed (the organisation number of a sole trader is a personal identity number, so the file holds personal data); otherwise the file says `Unnamed company`. `#KTYP` comes from the account type (asset, liability and equity, income, expense). `#IB` of the balance sheet accounts is the sum of the posted moves before the year; `#UB` and `#RES` are derived by `obop-sie`. Accounts with a non-numeric code are left out, and an exported line on one is an error.
- Voucher series and number: the `reference` if it looks like `A17` (letters, then digits without a leading zero), so an imported file round-trips; else the journal code and the digits at the end of the move number; else the next free number of the journal's series. A pair is never used twice.

`obop sie import FILE [--dry-run] [--post] [--default-account-types | --account-types FILE] [--db URL] [--token T]`
- Parses the file (an unbalanced voucher, a duplicate series and number, or a voucher on an account without `#KONTO` is refused before anything is written). New accounts (matched by code) need a confirmed type, as for the OAS importer, with the same table (`import_oas::types`, UNVERIFIED defaults by class; a file without `#KTYP` is typed from the class of the number). One journal per series (an existing journal with that code is used). Each voucher is a draft `accounting_move` with series and number in `reference` (`A17`), the text and `#VER` dates in `narration`, and its lines.
- Idempotent through the import map: accounts and moves are created with `bulk_create` and a `source` key (system `sie:<hash of company name and organisation number>`, table `voucher`, id `<year start>:<series>:<number>`), so a re-run creates nothing; a move without lines (an interrupted run) gets them. A move with the same reference in the same journal that is not in the map is refused.
- `--post` runs the real `accounting_move.post`, one voucher at a time in file order, as the signed-in person (needs `accounting.approver`; an agent or a scoped token is refused). JE numbers follow the file order and carry the year of the posting.
- After a real run the debit and credit totals per account of the imported moves, the line count and (with `--post`) the posted state are compared with the file; a difference fails the run with exit 1.
- Not imported and always listed: `#IB` opening balances (no opening entry model), `#UB`/`#RES` (derived), object tags (`#DIM`/`#OBJEKT`), other years, `#SRU`, `#VALUTA`, `#KSUMMA`. The bulk rate limit (`OBOP_BULK_PER_MINUTE`, 12 calls a minute) is waited out by the CLI, 1000 rows per call.

The typed commands `sie_exchange.export` (input `year`) and `sie_exchange.import` (input `content_base64`, `dry_run`, `default_account_types`, `account_types`) carry the file as base64 and run the same functions for REST and MCP callers as themselves. They have no `--post`, and an agent may only dry-run the import (see cli.md). Tests: `crates/obop/src/sie_tests.rs` (generated files only: round trip into a fresh database with an equal trial balance, dry run writes nothing, unbalanced file refused, re-run creates nothing).

## 5. Stage 4: the OAS importer (`obop import oas`)

An operator command: `obop import oas --from FILE.sqlite --documents DIR` (the first draft of this section said `--db`; `--db` is the obop database, as for every command). It reads the OAS SQLite file and the documents directory and writes obop records through the commands. It is CLI-only until the owner approves otherwise (the exemption list in the project instructions).

### 5.1 Table mapping

| OAS table | obop model | Notes |
|---|---|---|
| `company` | `accounting_se_company` | One row. `f_skatt` maps directly. `payment_info` and `contact_*` fold into the address and contact fields or are dropped (list them in the dry run). |
| `account` | `accounting_account` | `number` becomes `code`. OAS `kind` T, S, K, I (tillgång, skuld, kostnad, intäkt; the letters as OAS stores them) maps to `account_type`, and the mapping is lossy, because `accounting` has twelve types and OAS has four. The importer needs a mapping table the user confirms. The names are the user's own and are copied as they are. |
| `voucher` and `entry` | `accounting_move` and `accounting_move_line` | See 5.2. `entry.debit` and `credit` are öre, which equals minor units, so no scaling. `vat_code` has no field on a line (section 3), so it goes into the line label as `[S25]` or into a side table kept by the importer. `corrects_voucher_id` becomes a reversal pair. |
| `customer` | `party` (base) | `org_number`, `vat_number`, `country`, `email`, `address`. The `party` model decides which fields exist; the importer reads its metadata. |
| `invoice` and `invoice_line` | `invoicing` models | Depends on stage 5. Until then the importer skips invoices and says so. `vat_rate` (25, 12, 6) becomes basis points (2500, 1200, 600). `quantity` and `unit_price` are whole numbers in OAS; check the target field kinds before copying. |
| `employee` | `hr_employee` | `identity` is a personal identity number: class `confidential_hr`, never written to a log. `birth_year` is kept. |
| `pay_run` and `pay_line` | payroll package | Depends on stage 6. Skipped until it exists. |
| `document` and `document_meta` | `documents_document` with a file | `sha256` is checked against the file in the documents directory before upload. `vendor`, `amount`, `doc_date` become metadata fields if the target has them. |
| `bank_transaction` | `accounting_bank_statement_line` | `booking_date` to `date`, `amount` to `amount` (öre, positive in), `counterparty` to `counterparty_name`, `reference` to `label`. `status` booked maps to a matched line only if the booking voucher was imported. |
| `time_entry` | `timesheets_entry` | `start_utc` and `end_utc` are instants; the `tz` column is kept in a note. |
| `project` | `projects_project` | `rate_type` and `rate` map to the project's rate fields if present; `aliases` is dropped. |

### 5.2 Stated problems

1. **Posted history cannot go through `bulk_create`.** `bulk_create` creates records through the checks of `create`, and a governed state field such as `accounting_move.state` starts at its first option, `draft`. A historic posted entry would need `post`, which is an `accounting.approver` action that needs a human approval, one entry at a time, and is refused unless the entry balances (that part is fine). Importing thousands of vouchers through approvals is not workable. Options, in order of preference: a dedicated operator command that writes a batch of posted entries in one approved operation (core work, not X1); or import as drafts and post in bulk after one approval of the whole import. The first is the target; it needs a design and an owner decision.
2. **Historic numbers.** `accounting_move.number` is assigned at post by a gapless yearly series. An import cannot set it. The OAS number (series letter and number, for example `A17`) is kept in the `reference` field of the entry. Existing records are numbered only by `obop sequences backfill`, which numbers in creation order, so the import must create entries in OAS order.
3. **The hash chain is not portable.** OAS chains `prev_hash` and `hash` over vouchers. The hash covers OAS's own serialisation and ids. obop's records have different ids and no chain (section 9). The importer verifies the OAS chain before reading (and refuses a broken one unless told to continue), then drops the hashes and stores the final hash in the import record, so the origin can be shown later. The old chain does not continue in obop.
4. **Amounts.** OAS stores öre as integers. obop money is minor units. Equal, no conversion; the importer still refuses a value that does not fit.
5. **Dates and ids.** Dates are text `YYYY-MM-DD` in OAS and the same in obop. `bulk_create` takes ids of records, not names, and cannot point at another row of the same file, so the importer creates in dependency order and keeps a map from OAS id to obop id.
6. **Closed periods.** Entries inside a closed fiscal year need the year open at import time, then closed again by the approver. The importer opens nothing itself.
7. **Idempotency.** The import record holds the OAS database fingerprint and a row count per table, so a second run is refused or resumed, not repeated.

### 5.3 What is built (first version)

`obop import oas --from OAS.db [--documents DIR] [--dry-run] [--post] [--default-account-types | --account-types FILE] [--allow-broken-chain] [--db URL] [--token T]`

- **Read-only source.** The OAS file is opened read-only. The OAS hash chain is verified first with the same rule as the OAS `verify` route (numbering without gaps, `prev_hash`, the hash over the voucher); a broken chain is refused unless `--allow-broken-chain` is given. A voucher that does not balance is always refused.
- **Writes through the commands**, as the person who runs it, so every permission and limit applies. Nothing is written before the preflight (chain, balance, account types, receipt hashes) has passed.
- **Account types** (5.1) are confirmed by the person: the run refuses until `--default-account-types` accepts the built-in table (ranges by Swedish chart class, UNVERIFIED) or `--account-types FILE` (`number,type` lines) gives them. A dry run prints the table.
- **Posted history (5.2, items 1 and 2), decided as follows.** Every voucher is created as a draft `accounting_move` in OAS voucher order, with the OAS series and number (`A17`) in `reference`, the description, counterparty, who created and approved it, the corrected voucher and the attached receipts in `narration`, and a VAT code as `[S25]` in the line label. Without `--post` it stops there. With `--post` it runs the real `accounting_move.post` action, one voucher at a time in OAS order, as the importing person; an agent or a token narrowed by scopes is refused, and so is a person without `accounting.approver`. There is no bulk-posting command and no bypass of the approval rule: the approval is the importing person's own, once, for the whole run. JE numbers therefore follow the OAS order, but their `{year}` is the year of the posting (the clock of the run), not of the voucher date; the voucher date and the OAS number stay on the move. The operator command for a batch of posted entries (item 1, first option) is still not built.
- **Check after the run.** The per account debit and credit totals of the imported moves are compared with the totals OAS computes (the SQL of its `trial-balance` report, run on the same file); the line count and, with `--post`, the posted state of every move are compared too. A difference fails the run (exit 1) and says which accounts differ. The check is done by the importer against the file, not by calling an OAS server.
- **Idempotency.** A local ledger of records `oas_import_map` (source id from the company name and organisation number, OAS table and id, obop model and id, the OAS hash of a voucher) remembers what exists. A re-run creates only what is missing (so a failed or draft-only run can be continued, and `--post` can follow later), refuses if a voucher that was imported has changed in the file, and refuses a move that has the same reference in the same journal but is not in the ledger (an interrupted run or a hand entry). A run that changed something also leaves one `oas_import_run` record with the last hash of each series. This ledger is local to the importer; a shared source-key store is not used.
- **Receipts** become `documents_document` records (kind other, confidentiality internal) with the file as the attachment. Each file is found under `DIR/ab/abcdef...` (or `DIR/abcdef...`), hashed, and compared with the SHA-256 and the size OAS recorded; a mismatch or a missing file is refused before anything is written. The vendor, amount and date go into the summary text. Without `--documents`, receipts are not imported and the report says so.
- **Customers** become `party` records; organisation number, VAT number, address and country go into `notes` because `party` has no fields for them.
- **Company** becomes `accounting_se_company` when that package is installed. OAS has no fiscal year start or VAT period, so the importer sets month 1 and quarterly and says so; confirm them.
- **Journals.** One `accounting_journal` (kind general, code = the OAS series letter) is created per series, or an existing journal with that code is used.
- **Not imported, and always listed with row counts in the report** (dry run included): invoices and their lines (the `invoicing` model has no issue date, due date or VAT rate and numbers at post, so issued OAS invoices do not fit), employees, payroll, VAT configuration, bank transactions and matches, unapproved drafts, time entries, projects, the audit log and the OAS accounts and tokens. Columns without a target field (company contact name and phone, payment info, hashes) are listed too. Correcting vouchers stay ordinary moves with a note; they are not turned into reversal pairs.
- **Not done:** a closed fiscal year is not opened or closed by the importer (a post that the command refuses stops the run and a re-run continues; not tested against a closed period); no run against a real OAS database or a real Entra/production instance; `corrects_voucher_id` as `reverse` is not built.

## 6. Stage 5: invoices

OAS issues invoices with a gapless series `F`, a snapshot of the customer, a locked issued state and a draft ledger voucher. obop has the `invoicing` module for posting and numbering. Not built: the link from an issued invoice to an `accounting_move`, Swedish VAT on the invoice (the OAS limit is domestic customers at 25, 12 or 6 percent only), credit notes, EU and export invoices, and payment matching. Work order: the link to the ledger first, then credit notes, then EU and export cases once the codes are verified.

### 6.1 The first slice: `invoice_posting.create_move`

Built in `crates/obop/src/invoice_posting.rs` (typed command on REST and MCP, like `vat_computation.compute`). For a **posted** invoice (one with its number) it creates a **draft** `accounting_move` and its lines, as the caller:

- the caller names `journal`, `receivable_account`, `revenue_account`, `tax` (an `accounting_tax`) and `date` (an invoice has no date of its own); the VAT account is the tax's `tax_account_id`, or `vat_account` when given. Nothing is built in (no BAS account, rate or code, D34); a missing or unknown one is refused in words;
- lines: one receivable debit (net plus VAT), one revenue credit per invoice line carrying the `tax_id`, one VAT credit carrying the same `tax_id`, so `vat_computation.compute` (section 3.2) finds the base and the VAT once the move is posted and the tax has an `accounting_se_vat_code`. The command notes when no code has that `tax_id`;
- amounts are integer minor units; VAT is the net total times `rate_bp`, rounded half up once on the total (not per line); a zero rate gives no VAT line; credit notes (a negative line) and a non-positive total are refused;
- idempotent: a row of `invoice_posting` (invoice id, move id) is the link; a second run for the invoice is refused and writes nothing. The check and the link write are not one transaction: two simultaneous runs could both pass the check (a draft move would be doubled, never posted);
- governed: the move is a draft and gapless numbering is untouched. Posting is `accounting_move.post` (`accounting.approver`, approval). An agent may only `dry_run`;
- UNVERIFIED: nothing is checked against Swedish invoice requirements or the VAT act; the invoice currency is not compared with the ledger's; there is no customer snapshot, due date, payment term, OCR number or e-invoice. Gaps in the surfaces: docs/ISSUES.md K60.

## 7. Stage 6 to 8: payroll, receipts and bank, time

- **Payroll and AGI.** OAS computes gross, preliminary tax (entered per person), employer contributions by age class, a locked pay run that proposes a voucher, and an AGI XML file it says validates against Skatteverket's published schema. Planned as a new later package. Every rate, age class and the XML layout are UNVERIFIED and must be checked by the owner. Taxable benefits, absence and holiday pay, and the tax tables are not built in OAS either.
- **Receipts and bank.** OAS keeps receipts by content hash, imports a bank CSV, and proposes scored matches between bank lines and receipts. obop has `documents` and `accounting_bank_statement_line` with `match`, `reconcile` and `unmatch` actions. Not built: the scoring, the bank CSV import for Swedish formats and the booking of a bank line as a draft entry. A suggestion engine needs a command (X2), not a model.
- **Time tracking.** OAS has projects, activities, a timer and time entries billed once. The `timesheets` package covers time entries. Not built: the rule that an entry is billed at most once, and the invoice built from tracked hours.

## 8. BAS: what the package takes later

A BAS file supplied by the licensee is read by a command (not part of this PR) that creates `accounting_account` records and, with the user's confirmation, fills the `vat_account_id` of the VAT codes. The file stays on the licensee's machine; it is never committed or bundled. Until then the user types accounts in.

## 9. Immutability and the hash chain

OAS makes vouchers append-only with database triggers and a hash chain. In obop "immutable once posted" is not enforced by X1: the state is governed, but other fields of a posted entry can still be edited by anyone with the write group (MODULES.md section 10). The record history of D75 and the key in K5 are what this stage waits on; the first slice of D75 tracks nothing yet. Until that exists, the package is not suitable as the only copy of statutory books for a company that must show unchangeable vouchers. Note: the issue number K5 is as given in the task brief; `docs/ISSUES.md` K5 currently names a backup finding, so confirm the number before this is cited elsewhere.

## 10. Order of work

1. This PR: the package and this document. Check: `obop modules check packages/accounting_se.toml` and `--complete` with the Swedish labels.
2. Bundle the package in the catalogue (needs the labels in the other six languages, the catalogue tests updated, and a decision whether `requires_core` may be `^0.2.1` for a bundled package; see the PR notes).
3. `obop-sie` export, then import.
4. The operator command for posted history (5.2, item 1), then `obop import oas`.
5. A report builder (X2 or a command) that fills VAT report lines from ledger entries.
6. Invoices to the ledger, then payroll, receipts and bank matching.
7. An owner check of every box, code and rate against Skatteverket and the SIE specification, recorded as `verified` in the data.
