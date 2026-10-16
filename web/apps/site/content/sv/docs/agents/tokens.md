---
title: "Skapa en agenttoken"
description: "Skapa en agenttoken med begränsad behörighet i obop via REST (POST /api/auth/agent-token) eller med CLI (obop agent-token). Token visas en gång och kan bara snäva in ägarens behörighetsgrupper."
lead: "En agenttoken är en bärartoken som tillhör en person och har en lista med behörighetsgrupper. Skapa en per agent och per uppgift."
---

## Vad en token är

- Den tillhör personen som skapade den (dess _ägare_) och agerar med en delmängd av den personens grupper.
- Den visas **en gång**. obop lagrar bara en hash, så en borttappad token går inte att återskapa; skapa en ny.
- Den börjar med `obop_`. Behandla den som ett lösenord.
- Bara en person kan skapa token. En agent kan inte skapa en annan token.
- Den upphör inte att gälla, och slutar fungera när ägaren tas bort. Det finns inget kommando för att återkalla en enskild token ännu (se [Säkerhet](/docs/agents/safety/)).

## Välj scope

Scope är [behörighetsgrupper](/docs/concepts/permission-groups/). Titta på vad agenten ska göra och ge den bara de grupperna. Vanliga grupper i de inbyggda apparna:

| Grupp               | Låter agenten                                                    |
| ------------------- | ---------------------------------------------------------------- |
| `base.user`         | läsa och skriva kontakter (personuppgifter förblir maskerade)    |
| `crm.user`          | läsa och skriva leads och flytta dem genom säljflödet            |
| `sales.user`        | läsa och skriva offerter och ordrar                              |
| `invoicing.user`    | läsa och skriva fakturor och avbryta utkast                      |
| `invoicing.manager` | föreslå att en faktura bokförs (en person godkänner fortfarande) |
| `product.user`      | läsa produkter och priser                                        |

Installerade paket lägger till egna grupper, till exempel `helpdesk.agent`. [Appsidorna](/apps/) listar grupperna för varje app.

## Via REST

Logga in som person och be sedan om en token. Det kräver en körande instans; exemplen använder en lokal på port 8080.

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"your password"}'
```

Svaret är `{"token":"obop_..."}`: en sessionstoken som gäller i 12 timmar. Använd den som bärartoken för att skapa agenttoken:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-token \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"scopes":["base.user","crm.user"],"label":"triage-bot"}'
```

```json
{ "token": "obop_1e00c3..." }
```

`scopes` är obligatoriskt och måste ange minst en grupp som du själv har. `label` är fri text. På en hostad instans använder du dess adress i stället för `127.0.0.1:8080` och loggar in med e-postadressen från registreringen som användarnamn.

## Med CLI

Om du driftar obop själv kan du utfärda en token direkt från kommandoraden. Den talar direkt med databasen, så kör den där databasen går att nå:

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Lägg till `--db postgres://...` (eller sätt `OBOP_DATABASE_URL`) om instansen inte använder standardvärdet `sqlite://obop.db`.

## Förvara den säkert

- Lägg den i en miljövariabel eller ett hemlighetsvalv, inte i ett kodarkiv.
- Föredra en token per agent och per uppgift, så att du kan skilja dem åt och ta bort en utan att påverka de andra.
- Token i en klients konfigurationsfil kan läsas av allt som kan läsa filen.

## Fel

| Svar                                                                           | Betydelse                                       |
| ------------------------------------------------------------------------------ | ----------------------------------------------- |
| `400 {"error":"an agent token needs at least one scope"}`                      | `scopes` var tomt.                              |
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}` | Du bad om en grupp som du inte har.             |
| `400 {"error":"only a human user can issue agent tokens"}`                     | Du använde en agenttoken för att be om en till. |
| `401`                                                                          | Sessionstoken saknas, är fel eller har gått ut. |

Nästa steg: [anslut via MCP](/docs/agents/mcp/) eller [använd REST-API:et](/docs/agents/rest/).
