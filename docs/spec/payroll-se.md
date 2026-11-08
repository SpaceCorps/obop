# Payroll Sweden (`payroll_se`)

Status: a declarative X1 package, `packages/contrib/payroll_se/payroll_se.toml`, installable by upload and not bundled. It is stage 6 of [accounting-se.md](accounting-se.md). Everything in it is UNVERIFIED: nothing was compared with Skatteverket (employer contribution rates, age classes, tax tables, adjustment rules, the AGI specification). The package seeds no rate, no age class rule and no tax table, and it presents nothing as correct.

Source of the domain model: `crates/oas-server/src/payroll.rs` of open-accounting-sweden (MIT, the owner's own). The model is ported, no code.

## 1. What it is

The package depends on `hr` (`^0.3`) and needs core 0.2.1 (field limits, D90). It records what a preparer types in and carries a person's approval; it computes nothing.

| Model | Holds | OAS source |
|---|---|---|
| `payroll_se_employee` | A reference to `hr_employee`, personnummer, age class, tax basis (table, percent, adjustment), tax table and column, percentages in basis points, adjustment dates | table `employee` (name, identity, birth year) |
| `payroll_se_rate` | Employer contribution rate in basis points per age class, valid from and to, review state, who verified and when, source | table `payroll_rate` (key, value, source, verified) |
| `payroll_se_pay_run` | Period, pay date, computed totals (gross, tax, contribution, net), `rates_confirmed`, state draft, proposed, approved, paid | table `pay_run` |
| `payroll_se_pay_run_line` | Employee, gross, preliminary tax, age class, rate in basis points, rate row, employer contribution, computed net | table `pay_line` |
| `payroll_se_agi_submission` | Period, pay run, file reference, submitted date, receipt reference, state draft, prepared, recorded_submitted | none (OAS builds the XML on request) |

Money is in minor units (ore); rates are in basis points; there are no floats.

### Who may do what

- `payroll.preparer` reads and writes employees, runs, lines and AGI records and reads rates. An agent token holding it may create a run and `propose` it (agents only propose).
- `payroll.approver` writes the rate table, and is the only group that may `verify` a rate, `approve` a run, `record_paid` and `record_submitted`. Each of these has `approval = true` (a human approval, D18).
- `approve` asserts `rates_confirmed`: the approver states that every rate and tax figure on the lines was compared with Skatteverket and that the rate rows used are `verified`. X1 cannot check across records that every line's `rate_id` points at a verified row (MODULES.md section 10: no validation across records beyond assertions on actions), so this confirmation is a statement by a person, as with `boxes_confirmed` in `accounting_se`.
- Both groups are in `payroll.*`, which a mobile app is never offered (CONVENTIONS section 4).

## 2. Data classes, masking, retention and erasure

| Field | Class | Why |
|---|---|---|
| `personnummer` | `confidential_hr`, personal data | national id (CONVENTIONS section 5); masked for readers outside `payroll.preparer` and withheld from agents by D40/D41, no extra mechanism needed |
| tax basis, table, column, percentages, adjustment dates | `financial`, personal data | tax withheld for a named person (money paid to a person) |
| line gross, preliminary tax, net; run totals gross, tax, net | `financial`, personal data | pay of named people; the totals are recomputable from the lines, so the gate requires them to carry personal data too |
| employer contribution, run contribution total | `financial` | money, but a figure that is not a pay amount of one person on the line (the total is a sum) |
| `file_reference` of an AGI record | `financial` | refers to a file that carries personnummer |
| rate rows, age class labels, states, periods | `none` | not personal |

A value from which a protected value can be recomputed carries the strictest class of its inputs (D10); `personal_data` is set on every field the gate requires it on.

**Retention (deliberate, not decided here).** Pay records are usually subject to statutory retention periods (accounting and tax law). The package does not state a period: the period is a legal question that the owner and counsel must answer and that is UNVERIFIED here. X1 has no retention key, so the package neither deletes nor expires anything. `history` is left at its default `full` (the gate would refuse `none` for these classes), so changes to pay records are meant to be kept once D75 tracks them; nothing is tracked yet.

**Erasure (deliberate).** `delete` is plain in X1 and refuses while another record refers to the record (a pay line refers to its employee and run), so a person who has been paid cannot be deleted, which is intended while a retention duty may apply. The package has no `erase` command: X1 cannot anonymise in place (the `party.erase` command belongs to the owning module). Consequence: an erasure request after the retention period needs a command, not a model, and is not built. Until then the owner decides the handling of such a request by hand, outside this package. The `note` fields carry a warning not to put identity or health data in free text.

## 3. What X1 cannot express (and so is not built)

- **The AGI XML file.** X1 is declarative: it has no file generation, no schema validation and no outbound call (that is X2, a command). The package only records the period, the run, a reference to a file made elsewhere, the dates and the receipt. The XML layout and its validation against Skatteverket's published schema are NOT built and NOT checked.
- **Computing anything.** The employer contribution is an input per line because the formulas (the age classes, the youth reduction, caps per month) are exactly what is unverified; OAS marks its own youth formula as not reproducing an official example. The expression language could apply a basis-point rate (`a * b / 10000`), but the rate lookup by age class and period across records, and the reduction rules, are not expressible. Preliminary tax is not looked up from tables, and no tax table is shipped.
- **Age class from the personnummer.** There is no date or text arithmetic on a personnummer, so the age class is chosen by the user.
- **Checks across records.** That the rates used are verified, that a period has one line per employee, that a run's lines fit the employees' active flags, and that an approved run's lines are frozen: not expressible. OAS enforces the last with database triggers; in X1 the lines of an approved run can still be edited by anyone with `payroll.preparer` (MODULES.md section 10). The record history of D75 is what this waits on. Until then the package is not suitable as the only copy of statutory payroll records.
- **A draft ledger voucher** from an approved run, as OAS creates. That needs a command that writes `accounting_move` records (X2).
- **Not in OAS either:** taxable benefits, absence and holiday pay, corrections to an earlier month, tax tables.

## 4. Checks run

`cargo run -- modules check packages/contrib/payroll_se/payroll_se.toml` and with `--complete` (Swedish labels in `labels/sv/payroll_se.toml`), and `cargo run -- modules describe --json` on the file. No pack was installed in a running server and no test exercises the package; the Swedish labels are machine-drafted.
