---
title: "Data classes and masking"
description: "Every obop field has a data class. Agents see personal data as stable placeholder tokens and sensitive classes not at all, and can write tokens back without seeing the real value."
---

## Data classes

Every field in every module declares a **data class**. It says how sensitive the value is, and it drives masking for agents. A field without a class cannot be installed.

| Class | Use for | Examples |
|---|---|---|
| `none` | Not personal and not otherwise sensitive. Must be written out. | Product code, state, quantity |
| `public_directory` | Directory data every internal user may see. | Employee name, work email |
| `internal` | Business data about people and work. | Ticket requester, site address |
| `confidential_hr` | Personal employment facts that are not money. | Contract type, national id |
| `special_category` | Health, union, biometrics. | Sick-leave flag |
| `financial` | Money owed to or held for a named person; bank accounts. | Salary, IBAN, tax identifier |
| `secret` | Credentials and keys. | API keys |

A field can also be marked `personal_data = true`: it is about an identifiable person. A personal-data field may never be class `none`.

`confidential_hr`, `special_category` and `financial` are the **sensitive classes**. `secret` is write-only.

## What people and agents see

| Reader | `none`, `internal`, `public_directory` | Personal data | Sensitive classes | `secret` |
|---|---|---|---|---|
| A person | in clear | in clear | in clear, if their group allows | never returned |
| An agent | in clear | masked as a token | withheld | withheld |

A person sees real values under their normal permissions. An agent token (see [Agents](/docs/agents/)) gets:

- **Masked** values for personal data, such as `NAME_2` or `EMAIL_1`. A token is stable for the same value within one session of that agent token, so the agent can still tell that two records mention the same person.
- **Withheld** fields for sensitive classes. The response lists the field names that were withheld (`denied` in REST, `withheld_fields` over MCP).

## Writing tokens back

An agent can pass a token in a write. The server swaps it for the real value if the agent could have read the record, so an agent can copy a contact into a new lead without ever seeing the contact's name or email. A token the server does not know is refused with `invalid token`.

Tokens are kept in memory per agent token and per server process. A restart, or a different agent token, starts with a fresh table, so an old token stops resolving.

## Filtering and searching

Filters and sorting run on the real values before masking. To stop an agent guessing masked values, it may only filter and sort on fields it can read in clear. Asking for any other field fails with the same error as a field that does not exist.

## Limits, honestly

- Masking is by field class and the personal-data flag. It does not look inside free text: a note typed into a `none` field is shown as it is. Classify free-text fields that may contain personal data as personal data.
- Tokens are not stable across restarts or across agent tokens.
- Erasure: Contacts has an `erase` action that anonymises a record in place; other apps do not yet.

See also [Masking tokens](/docs/agents/masking/) and [Security and privacy](/docs/reference/security/).
