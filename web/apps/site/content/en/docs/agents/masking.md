---
title: "Masking tokens"
description: "How an obop agent reads personal data as placeholder tokens such as EMAIL_1 and passes them back to refer to a value it never sees. Stability, limits and what to tell the agent."
lead: "Agents can work with people's data without seeing it. This page explains how tokens behave so you can instruct an agent properly."
---

## What an agent receives

For a field that holds personal data, the server replaces the value with a token built from the field name and a counter, such as `EMAIL_1`, `NAME_2` or `CONTACT_EMAIL_7`. Fields in a sensitive data class (confidential HR, special category, financial) or in the secret class are left out and named in `denied`, over REST and over MCP.

```json
{
  "denied": ["party.tax_id"],
  "value": { "email": "EMAIL_4", "id": "01a14650-2667-...", "name": "NAME_5", "notes": "NOTES_6" }
}
```

## Rules of the tokens

- **The same value in a field of the same name gets the same token** within one table: one agent token (or MCP session). Two records with the same email in `email` show the same token, so the agent can see they match; the same email in `contact_email` gets another token. Each `obop call` starts a new table.
- **Counters are shared**, so a token's number says nothing about the record. Do not parse it.
- **They are not stable across agent tokens or MCP sessions.** A different agent token or a new MCP session starts a new table, and a table is forgotten 24 hours after its last change (REST), 2 hours after the session's last request (MCP over HTTP) or when `obop mcp` stops; a restart of the server does not lose a table over REST or MCP on HTTP. A token an agent remembers from yesterday may not resolve today.
- **They resolve only in the table that showed them.** The server does not check that the agent can still read the record a token came from; any caller with at least one permission group can pass back a token it was shown.

## Passing tokens back

When the agent writes, it can use a token where the value would go. The server replaces it before saving:

```bash
curl -s -X POST http://127.0.0.1:8080/api/lead/create \
  -H "Authorization: Bearer $AGENT_TOKEN" -H 'content-type: application/json' \
  -d '{"title":"Support renewal","contact_name":"NAME_5","contact_email":"EMAIL_4"}'
```

A person opening that lead sees the real name and email. The agent only ever handled the tokens. A token is replaced only when it is the whole value of a field and is in the agent's table. A token the server does not know (for example one from a forgotten table) is not refused: it is saved as the literal text, so check the record.

If the agent types a real value instead (for example because a user told it), the value is saved as given and shown to the agent as a new token afterwards.

## Filtering by a masked field

An agent cannot filter or sort on a masked or withheld field. The request fails as if the field did not exist: `unknown field party.email`. This is on purpose: otherwise an agent could find a hidden value by guessing and checking.

## What to put in the agent's instructions

> Personal data is masked. You will see placeholders such as NAME_1 or EMAIL_2. Never try to guess or reconstruct the real value. To use a person's details in a new record, copy the placeholder. If a field is listed as withheld, tell me you cannot see it instead of asking for it.

## What masking does not do

- It does not read free text. If a person types a customer's phone number into a note, that note is shown as it is unless the field is marked as personal data.
- It protects fields, not reasoning. An agent that is told a real value in chat can repeat it. Keep real personal data out of prompts.
- It is not a legal guarantee. Treat it as one layer; see [Security and privacy](/docs/reference/security/).

Background: [Data classes and masking](/docs/concepts/data-classes-and-masking/).
