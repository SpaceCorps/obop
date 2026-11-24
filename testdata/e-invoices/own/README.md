# obop's own e-invoices: fictitious test data

Everything here is **invented**. Every file says `FAKE TEST DATA` in an XML comment.

- Company names end in "(TEST)", and email domains are `.example`.
- German VAT ids are `DE0000000xx`.
- Swedish organisation numbers are `99000000xC`: the check digit is valid, but the 3rd and 4th digits are `00`, which no real organisation number has (and no personal number has month `00`). Swedish VAT ids are `SE` + that number + `01`.
- Bankgiro numbers (`9900000x`) are invented. Never pay to one.
- The IBANs in 02 and 03 (`DE02100100100006820101`, `DE02120300000000202051`) are widely published example IBANs with valid check digits, not invented ones. Never pay to them either.

Nothing here is a real business or a real invoice.

| File | Expect | What it tests |
|---|---|---|
| 01-valid-cii-one-line.xml | ok | XRechnung CII, one line, 19 % VAT |
| 02-valid-ubl-consulting.xml | ok | XRechnung UBL syntax |
| 03-valid-cii-two-vat-rates.xml | ok | 19 % and 7 %, two tax breakdowns |
| 04-valid-cii-credit-note.xml | ok | credit note (type 381) referring to RE-2026-4711 |
| 05-error-wrong-total.xml | error | grand total 1200.00 ≠ 1000.00 + 190.00 (BR-CO-15) |
| 06-error-no-due-date-no-terms.xml | error | amount due without due date or payment terms (BR-CO-25) |
| 07-duplicate-of-01.xml | duplicate | same seller VAT id and number as 01 |
| 08-not-an-einvoice.xml | error | valid XML, but not an invoice |
| 09-broken-xml.xml | error | cut-off file: the reader must fail cleanly |
| 10-valid-peppol-se-25.xml | ok | Peppol BIS 3.0 for Sweden, SEK, 25 %, bankgiro, "Godkänd för F-skatt" |
| 11-valid-peppol-se-12.xml | ok | 12 % (hotel nights) |
| 12-valid-peppol-se-6-and-25.xml | ok | 6 % and 25 % in one invoice; the 6 % group rounds 14.994 to 14.99 once per group |
| 13-valid-peppol-se-reverse-charge.xml | ok | domestic reverse charge for construction: category AE, 0 %, "Omvänd betalningsskyldighet" (VATEX-EU-AE) |

`expected.json` says, per file, what a reader should return: number, dates, seller, buyer, lines, and net, VAT, gross and amount due **in minor units** (öre, cents). The Swedish entries also carry each line's `vat_category`, the seller's `org_number` and a `vat_breakdown` per VAT group, with the exemption reason for AE.

**What was checked.** Files 01 to 07 and 10 to 13 validate against the XML schemas: UBL 2.1 from OASIS for 02 and 10 to 13, and the CII D16B schema of the CEN artefacts 1.3.17 for the CII files. The amounts agree with `expected.json`, except in 05, which is wrong on purpose. **None of them has been run through the KoSIT validator or the OpenPeppol Schematron rules, including the Swedish SE-R rules.** That is issue #7. For cases validated by an authority, use `../kosit/`.

Written for obop by its contributors; Apache-2.0 like the rest of the repository.
