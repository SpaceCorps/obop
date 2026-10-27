# Signing with DocuSeal

- **Status:** implemented, experimental. Tested against a local stand-in for DocuSeal, not yet against a live DocuSeal server.
- **Scope:** how a Sign request is sent to a DocuSeal server and how DocuSeal's answer comes back. For the connector framework itself see [CONNECTORS.md](CONNECTORS.md); for the package format see [MODULES.md](MODULES.md).

## What it is

Two parts:

1. The **`docuseal` provider** in the connectors catalogue. Outbound it creates one DocuSeal submission per record (`POST /api/submissions`, key in `X-Auth-Token`) for a template you name. Inbound it takes DocuSeal's webhook and writes it into a model you name.
2. The **`sign_docuseal` package** (bundled, `packages/sign_docuseal.toml`). It depends on `sign` and adds the model `sign_docuseal_event`, the target of the webhook.

DocuSeal keeps the document, the signed PDF and the audit trail. obop keeps the request, the event and a link.

## What it does not do

- **It does not sign for you.** An event never moves a `sign_request`. A person still runs the `sign` action, which needs approval (D18, the `sign` package). This is on purpose: the platform cannot yet lock a signed record, and agents may only propose.
- **It is a simple electronic signature** (eIDAS), the kind DocuSeal produces. It is not a qualified signature and it is not BankID. D39 keeps qualified signatures for QTSP connectors.
- **The webhook is not integrity-protected.** DocuSeal cannot sign its deliveries, so the check is a shared secret in the `x-docuseal-secret` header (compared in constant time), and an identical body is accepted once. A party that knows the secret can send any body. Treat the events as hints that a person confirms in DocuSeal before recording a signature.

## Licence

DocuSeal's open edition is AGPL software. It runs as its own service next to obop and obop talks to it over HTTPS; nothing of it is bundled, linked or copied, so the Apache-2.0 licence of obop and the rule against copyleft components in core (D39) are untouched. Use only DocuSeal's free features (OR-2): its paid tier (SSO, identity verification, branding) is not used and not needed.

## Set it up

1. **Run DocuSeal.** The official image is `docuseal/docuseal`. It keeps its data in `/data` (SQLite by default; PostgreSQL through `DATABASE_URL`), so give it a persistent volume. On Sliplane there is a one-click DocuSeal preset that does this; put it on HTTPS and note its address.
2. **Make the template.** In DocuSeal create the NDA or contract as a template with one signing party. Note the template id (the number in its address) and the party's role name.
3. **Create an API key** in DocuSeal, Settings, API.
4. **Install the package.** Customize, Modules, Marketplace, `sign_docuseal` (this installs `sign` first).
5. **Create the connection** (Connectors app, provider DocuSeal):
   - `url`: `https://<your docuseal>/api/submissions`
   - `api_key`: the key from step 3 (sealed, never shown again)
   - `signing_secret`: a long random value you choose (at least 16 characters)
   - `template_id`, `role`: from step 2 (`role` defaults to `First Party`)
   - `events`: `sign_request.send` to send when a request is sent
   - `target_model`: `sign_docuseal_event`
   - `resolve_refs`: `true`
   - `mapping`: `{"event_type": "event", "data.external_id": "request_ref", "data.submission_id": "submission_id", "data.status": "status", "data.completed_at": "completed_at"}`
   - Field allowlist: `sign_request`: `signer_name`, `signer_email`; `sign_docuseal_event`: `event`, `request_ref`, `submission_id`, `status`, `completed_at`
6. **Let the signer's name and email leave in clear.** DocuSeal has to email the signer, and the default policy pseudonymises personal data. Set the `internal` class to `allow` for this connection, record the agreement with whoever runs DocuSeal, and check the privacy preview. Without this DocuSeal gets `psn_...` in place of an address and refuses the submission.
7. **Add the webhook in DocuSeal**, Settings, Webhooks: the URL is the connection's webhook address (`POST /hooks/in/{connection}`, shown on the connection), with a custom header `x-docuseal-secret` holding the secret from step 5. Select the form and submission events you want.
8. Test, then connect.

## How a request finds its event

The record's reference goes to DocuSeal as the submitter's `external_id`. By default that reference is a pseudonym (`psn_...`, a keyed HMAC) that cannot be turned back into a record. With `resolve_refs = true` the inbound side looks the pseudonym up in the instance's own pseudonym map, which the erasure hook already clears, and stores the record id in `request_ref`. After a subject is erased the lookup fails and the event keeps only the pseudonym. `sign_docuseal_event.request_ref` equals the id of the `sign_request`.

The option exists only on connections that set it. The general rule that pseudonyms have no way back (CONNECTORS.md section 2) is unchanged, and the test that asserts it still passes.

## Tests

- `webhook::tests::header_scheme`: the secret check, the replay id, a wrong and a missing secret.
- `connectors::tests::docuseal_gets_a_submission_with_the_key_header_and_reports_back_with_a_shared_secret`: the submission body (template id, role, name, email, external id), the `X-Auth-Token` header and no bearer header, the event creating a record whose `request_ref` is the original party id, and the refusals (replay, wrong secret, no secret).
- `catalog` tests: the entry is `generic_adapter`, so the catalogue says it is not tested against DocuSeal itself.
- `obop-pkg` marketplace tests and `modules check --complete packages/sign_docuseal.toml`: the package and its labels in all six languages.

## Not done

- A run against a live DocuSeal server. Until one is done the provider stays `generic_adapter`.
- A button on the request that opens the DocuSeal signing link or the signed document.
- Pulling the signed PDF into the Documents app.
- Sliplane provisioning of DocuSeal from the control plane.
