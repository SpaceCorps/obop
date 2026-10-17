---
title: "Connectors and compliant data flows"
description: "How obop connects other systems and lets people connect their own accounts: a recorded legal basis, a field allowlist, pseudonyms by default, withdrawable consent, a log without payloads and a generated processing register. Technical controls that support GDPR compliance, not legal advice."
lead: "Connecting a system is where an instance can leak. The Connectors app puts one gate in front of every flow in or out, so the safe path is the only path."
---

> **This supports compliance, it does not provide it.** The controls below help you meet the GDPR. They are not legal advice and not a certification. You still decide the purposes, the legal basis, the agreements with recipients and the assessments, and you are accountable for them.

## What you can connect

Open **Connectors** from the app launcher. The **Catalog** lists every service and says how much of it exists:

| Badge           | Meaning                                                                                                                                |
| --------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Working         | Implemented and tested end to end against a local counterpart: webhook (send), webhook (receive), REST API (pull), CSV export          |
| Generic adapter | A generic engine configured for the service (Slack, Stripe, GitHub, Zapier, Make, n8n). Tested locally, not against the service itself |
| Definition only | Metadata and setup notes (Google and Microsoft calendar and contacts, which need your own OAuth client). Nothing runs                  |

Facts about a recipient that were not checked say "verify with the provider". An administrator enables a service, and records where the agreement with the provider is kept, before anyone can connect it.

## The Connect wizard

1. **Service.** Pick one that is enabled.
2. **Settings.** The address and the credentials. Credentials are write-only: they are stored encrypted and never shown again, not to you, not to an administrator, not to an agent.
3. **Fields.** Choose the fields that may leave, model by model. Each field shows its data class and what happens to it: sent as a pseudonym, sent as is, or never sent. An empty choice sends nothing. The **privacy preview** shows one real record exactly as it would leave.
4. **Purpose and legal basis.** Say what the data is used for, choose the basis, and see who receives it and which data classes would flow. Confirming records a consent record.
5. **Test and connect.** A test sends no personal data. Connect starts the flows.

## What the policy does

| Data class                                   | By default                                                                                                                                                                          |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Secrets                                      | Never leave                                                                                                                                                                         |
| Confidential HR, special category, financial | Withheld. To release one, the owner raises that class and records an impact-assessment note (who and when are stamped by the server), and the service must be allowed to receive it |
| Other personal data                          | Sent as keyed pseudonyms, stable for one connection and meaningless on another                                                                                                      |
| Not personal                                 | Sent as is                                                                                                                                                                          |

Record ids never leave; each row carries a pseudonym instead. Every flow writes one log row with the field names, the treatments and a hash of the payload, never the payload. Rate and volume caps apply per connection, and logs are kept for the retention you set.

## People's own accounts, and consent

A person with the Connectors permission connects their own accounts from the services an administrator enabled for people. They see only their own connections. Withdrawing consent is one click: the connection is revoked and its credentials, tokens and pseudonym maps are removed. When a contact is erased, the pseudonym maps and tokens that pointed at it are dropped and the erasure is logged.

## Agents

Everything here is a command, so agents get the same tools over MCP and REST. Connecting, pausing, resuming, retrying and revoking a connection, running it, withdrawing a basis and any policy that releases more than the default are proposals that a person approves. Credentials never appear in a tool result.

## For administrators

The **Processing register** lists every connection with its recipient, purpose, basis, categories of personal data, transfer basis, agreement and retention, and exports as CSV. It is a starting point for your record of processing activities, not a replacement. The **Compliance** page checks what is configured: encryption key source, agreements, retention, bases, acknowledgements and fields sent in clear. Set `OBOP_SECRET_KEY` (at least 32 characters) for production, `OBOP_PUBLIC_URL` for webhook addresses, and run `obop connectors purge` or let the server do it daily.

The design, the treatment table, how to add a provider, the mapping to GDPR articles 5, 6, 7, 25, 28, 30 and 32 with what remains your job, and the threat model with its tests are in `docs/CONNECTORS.md` and decision D54.
