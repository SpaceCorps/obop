---
title: "Skapa en agenttoken"
description: "Skapa en agenttoken med begränsad behörighet i obop i back office, via REST (POST /api/auth/agent-token) eller med CLI (obop agent-token). Token visas en gång och kan bara snäva in vad ägaren får göra."
lead: "En agenttoken är en bärartoken som tillhör en person och har en lista med behörighetsgrupper. Skapa en per agent och per uppgift."
---

## Vad en token är

- Den tillhör personen som skapade den (dess _ägare_) och agerar med en delmängd av den personens grupper.
- Den visas **en gång**. obop lagrar bara en hash, så en borttappad token går inte att återskapa; skapa en ny.
- Den börjar med `obop_`. Behandla den som ett lösenord.
- Bara en person kan skapa token. En agent kan inte skapa en annan token.
- Den upphör inte att gälla. Ägaren kan återkalla den när som helst (se [Lista och återkalla](#lista-och-aterkalla)). Den fungerar dessutom inte medan ägarens konto är inaktiverat, och slutar fungera för gott när ägarens konto avslutas (offboarding).

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

## I back office

Öppna _Anpassa, Säkerhet_ och gå till **Agenttokens**. Skriv en etikett (programmet eller agenten som ska använda token), kryssa i grupperna och välj **Skapa agenttoken**. Token visas en gång, med en knapp **Kopiera**; välj **Klar** när du har sparat den. Avsnittet listar också de token du har skapat, var och en med **Återkalla**.

## Via REST

Logga in som person och be sedan om en token. Det kräver en körande instans; exemplen använder en lokal på port 8080.

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"your password"}'
```

Svaret är `{"token":"obop_..."}`: en sessionstoken som gäller i 12 timmar. Om tvåfaktorsinloggning är påslagen för kontot blir svaret i stället `{"mfa_required":true,"mfa_token":"..."}`; skicka `{"mfa_token":"...","code":"123456"}`, med en kod från din autentiseringsapp, till `POST /api/auth/login/mfa` för att få sessionstoken. Använd sessionstoken som bärartoken för att skapa agenttoken:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-token \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"scopes":["base.user","crm.user"],"label":"triage-bot"}'
```

```json
{ "token": "obop_1e00c3..." }
```

`scopes` är obligatoriskt och får inte vara tomt. Det anger grupper som står på ditt eget konto. En administratör (en person som har `platform.admin`) får ange vilken grupp som helst som instansen har, eftersom den gruppen redan tillåter alla. Token bär ändå bara de grupper du anger: `*` är ingen grupp och avvisas, och en grupp som ingen installerad modul deklarerar avvisas också. `label` får vara högst 80 tecken på en rad. På en instans som ett kontrollplan har skapat använder du dess adress i stället för `127.0.0.1:8080` och loggar in med e-postadressen från registreringen som användarnamn.

## Med CLI

Om du driftar obop själv kan du utfärda en token direkt från kommandoraden. Den talar direkt med databasen, så kör den där databasen går att nå:

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Lägg till `--db postgres://...` (eller sätt `OBOP_DATABASE_URL`) om instansen inte använder standardvärdet `sqlite://obop.db`. Scope följer samma regel som via REST, för den användare du anger. Kommandoraden kan ännu inte lista eller återkalla token.

## Lista och återkalla

Du ser och återkallar dina egna agenttoken under _Anpassa, Säkerhet, Agenttokens_ eller via REST med din sessionstoken:

```bash
curl -s http://127.0.0.1:8080/api/auth/agent-tokens \
  -H "Authorization: Bearer $SESSION_TOKEN"
```

```json
{
  "tokens": [
    {
      "id": "5f0c...",
      "label": "triage-bot",
      "scopes": ["base.user", "crm.user"],
      "created_at": 1794650000
    }
  ],
  "delegable": ["base.user", "crm.user"]
}
```

`tokens` innehåller aldrig en token, bara vad var och en heter och får göra. `delegable` är listan över grupper som du får lägga på en ny. För att återkalla en token skickar du en POST till dess `id`:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-tokens/5f0c.../revoke \
  -H "Authorization: Bearer $SESSION_TOKEN"
```

Token slutar fungera direkt. Bara ägaren kan göra detta: en administratör kan inte lista eller återkalla en annan persons token en och en (om kontot inaktiveras eller avslutas slutar alla fungera). Både skapandet och återkallandet av en token skrivs till säkerhetsloggen, utan själva token.

## Använd token för MCP

Samma token ansluter en MCP-klient till instansens fjärr-MCP-server, `https://YOUR-INSTANCE/mcp`: skicka den som `Authorization: Bearer obop_...` (klienter har ett `headers`-fält eller en variabel för bärartoken till detta). En token som skapats med CLI eller via REST fungerar för båda; det finns inget MCP-specifikt att skapa. Tänk på följande:

- Skapa en separat token per klient och ge den bara de scope som klienten behöver. MCP-verktygslistan är exakt de kommandon som scope tillåter.
- En sessionstoken från inloggning fungerar också, men den går ut efter 12 timmar (en agenttoken gör det inte), och den agerar som du: personuppgifter maskeras inte och kommandon som kräver godkännande körs utan godkännande. Använd en agenttoken för en agent.
- Token måste skickas med varje förfrågan, inte bara den första; klienter gör detta när den väl finns i deras konfiguration.
- Klienter som stöder inloggning i webbläsaren (OAuth) behöver ingen inklistrad token: de ansluter med en inloggning och får en token som gäller en timme och bara fungerar på `/mcp`. Vi har ännu inte prövat det med Claude Desktop, claude.ai eller Claude Code; se [Anslut via MCP](/docs/agents/mcp/).

## Förvara den säkert

- Lägg den i en miljövariabel eller ett hemlighetsvalv, inte i ett kodarkiv.
- Föredra en token per agent och per uppgift, var och en med bara de scope som agenten behöver.
- Token i en klients konfigurationsfil kan läsas av allt som kan läsa filen.

## Fel

| Svar                                                                               | Betydelse                                                                                                   |
| ---------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `400 {"error":"an agent token needs at least one scope"}`                          | `scopes` var tomt.                                                                                          |
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}`     | Du bad om en grupp som inte står på ditt konto, och du är inte administratör.                               |
| `400 {"error":"nope.user is not a registered permission group, ..."}`              | En administratör bad om en grupp som ingen installerad modul deklarerar.                                    |
| `400 {"error":"only a human user can issue agent tokens"}`                         | Du använde en agenttoken för att be om en till.                                                             |
| `401`                                                                              | Sessionstoken saknas, är fel eller har gått ut.                                                             |
| `403 {"error":"two-factor authentication must be set up before you can continue"}` | Ditt konto måste först ställa in tvåfaktorsinloggning; tills dess kan den här sessionen bara ställa in den. |

Nästa steg: [anslut via MCP](/docs/agents/mcp/) eller [använd REST-API:et](/docs/agents/rest/).
