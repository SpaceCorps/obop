# E-invoice samples

Test data for reading, validating and building EN 16931 e-invoices (XRechnung, Peppol BIS Billing 3.0, ZUGFeRD / Factur-X). `provenance.json` names each set's authority, URL, licence, tag, commit and retrieval date, with a SHA-256 for every committed file (docs/LOCALIZATION.md 1.6). `crates/obop/tests/e_invoice_samples.rs` checks it. That test lives there until the `edi` crate exists.

| Folder | Set | Licence | Committed? |
|---|---|---|---|
| `kosit/` | KoSIT XRechnung testsuite `v2026-08-31`: 48 of the 72 business cases, the ones under 100 KB, in UBL (`*_ubl.xml`) and CII (`*_uncefact.xml`) | Apache-2.0 (`kosit/LICENSE`, attribution in `NOTICE`) | yes, unchanged copies |
| `own/` | obop's own invoices, all **fictitious**: 9 German (XRechnung CII and UBL, valid and broken) and 4 Swedish (Peppol BIS 3.0, SEK) | Apache-2.0 | yes |
| `target/e-invoice-samples/cen/` | CEN/TC 434 validation artefacts `validation-1.3.17`: 32 examples in UBL and CII | EUPL-1.2 (copyleft, not on the `deny.toml` allow-list) | no, fetched |
| `target/e-invoice-samples/peppol/` | OpenPeppol BIS Billing 3.0 `3.0.22`: 26 examples and snippets | unclear: no licence file, README note or header found at this tag, so this is a question for counsel | no, fetched |

## The fetched sets

```sh
deploy/fetch-einvoice-samples.sh
```

The script downloads every file listed in `fetch.txt` from its pinned commit into `target/e-invoice-samples/` (or `$OBOP_EINVOICE_SAMPLES_DIR`), and keeps a file only when its SHA-256 matches. Locally, a test that needs these files skips and prints this command. CI sets `OBOP_REQUIRE_EINVOICE_SAMPLES=1`, so there a missing file fails the test. A file with the wrong bytes always fails.

## Changing a set

- **New tag of a third-party set.** It must be past the pin cooldown (docs/DECISIONS.md D23). Update `tag`, `commit` and `retrieved` in `provenance.json`, and the lines of `fetch.txt` or the files under `kosit/`, together with their hashes. The test fails until they agree.
- **New own invoice.** Mark it `FAKE TEST DATA` in an XML comment, add its entry to `own/expected.json`, and list its hash in `provenance.json`.
- **Never commit** a file from `cen` or `peppol`, or any file whose licence is not Apache-2.0 or compatible with it (OR-1).
