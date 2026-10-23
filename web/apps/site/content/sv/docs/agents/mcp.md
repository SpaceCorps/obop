---
title: "Anslut via MCP"
description: "Anslut Claude Code, Cursor, VS Code, Windsurf, Codex eller Gemini CLI till en obop-instans via URL med fjärr-MCP-servern (Streamable HTTP, bärartoken), eller kör obop mcp över stdio. Exakt konfiguration för varje klient."
lead: "Varje obop-instans har en MCP-server på /mcp. Rikta en klient mot URL:en med en agenttoken, så blir varje kommando som token får använda ett MCP-verktyg: ingen lokal binärfil behövs."
---

## Fjärr-MCP: anslut via URL

En instans tillhandahåller [Model Context Protocol](https://modelcontextprotocol.io/) över **Streamable HTTP** på `https://YOUR-INSTANCE/mcp`. En hostad instans (till exempel på Sliplane) och en lokal `obop serve` har båda den; inget behöver slås på.

- **Autentisering:** inloggning i webbläsaren (OAuth, se nedan) för Claude Desktop, claude.ai och Claude Code, eller `Authorization: Bearer <token>` i varje förfrågan med en agenttoken. I båda fallen kontrolleras token som för REST. En saknad eller felaktig token får `401` med `WWW-Authenticate: Bearer resource_metadata="https://YOUR-INSTANCE/.well-known/oauth-protected-resource/mcp"`, som MCP:s auktoriseringsspecifikation kräver; det dokumentet nämner instansens auktoriseringsserver (`/.well-known/oauth-authorization-server`). En token som utfärdats vid inloggningen gäller bara för `/mcp`, inte för `/api`.
- **Protokollrevisioner:** `2025-11-25`, `2025-06-18` och `2025-03-26`, förhandlade i `initialize`. Den nyaste revisionen, `2026-07-28`, är tillståndslös och fungerar annorlunda; obop implementerar den inte ännu. En klient som talar båda faller tillbaka på `initialize` när den får ett vanligt `400`, vilket är vad obop skickar för en versionsheader som inte stöds.
- **Sessioner:** `initialize` returnerar ett `Mcp-Session-Id`; skicka det i varje senare förfrågan. Sessionen behåller maskeringsvalvet, så en maskeringstoken som `NAME_1` betyder samma sak i varje anrop i sessionen och du kan skicka tillbaka den. En annan session startar ett nytt valv. `DELETE /mcp` avslutar en session; en okänd eller utgången session svarar `404`, och klienten börjar om med `initialize`. En session tillhör den token som öppnade den och går ut efter två timmar utan förfrågan.
- **Svaren är vanlig JSON.** Det finns ingen SSE-ström från server till klient (`GET /mcp` svarar `405`), och JSON-RPC-batchar accepteras bara i `2025-03-26`, den enda revision som har dem.
- **Gränser:** förfrågningskroppar upp till 1 MiB (`413`), 600 förfrågningar per minut och token (`429` med `Retry-After`, kan justeras med `OBOP_MCP_RATE_PER_MINUTE`), 25 öppna sessioner per token.
- **Webbläsare:** en förfrågan med en `Origin`-header avvisas med `403` om inte ursprunget är instansens egen publika URL (`OBOP_PUBLIC_URL`), samma ursprung på en loopback-värd, eller finns i `OBOP_ALLOWED_ORIGINS` (kommaseparerad). Det skyddar lokala instanser mot DNS rebinding. Klienter som inte är webbläsare skickar ingen `Origin`. Bakom en proxy sätter du `OBOP_PUBLIC_URL` (till exempel `https://acme.example.com`) så att upptäcktsdokumenten anger den publika adressen.

### Anslut med inloggning i webbläsaren (OAuth)

Claude Desktop, claude.ai och Claude Code kan ansluta till en instans utan inklistrad token. Lägg i appen till en egen koppling (custom connector) med adressen `https://YOUR-INSTANCE/mcp` (Claude Code: `claude mcp add --transport http obop https://YOUR-INSTANCE/mcp`, sedan `/mcp` och välj att autentisera). Appen hittar inloggningen själv och öppnar en sida på **din instans** i webbläsaren. Där:

1. loggar du in med ditt obop-konto och anger din tvåfaktorskod om kontot har en (en passkey fungerar också, som andra steg eller ensam om du tillåtit det under Säkerhet);
2. ser du vilken applikation som frågar och till vilken adress webbläsaren återvänder;
3. kryssar du i de behörighetsgrupper som ska lämnas över, väljer hur länge åtkomsten gäller (1, 7, 30 eller 90 dagar) och trycker på **Tillåt**.

Vad det betyder:

- **Appen kan bara begränsa dig.** Du erbjuds de grupper du själv har; appen agerar som du med de du kryssar i, aldrig med fler ([agenttokens](/docs/agents/tokens/) fungerar likadant). Dess åtgärder registreras som din agents.
- **Godkännanden kräver fortfarande en person.** Ett steg som kräver godkännande väntar i obop på en människa; appen kan inte godkänna sina egna förslag.
- **Kortlivade tokens.** Appen får en åtkomsttoken som gäller en timme och en förnyelsetoken som används en gång och ersätts varje gång. Om en förnyelsetoken används två gånger avslutas hela anslutningen. En åtkomsttoken från den här inloggningen gäller bara för `/mcp`.
- **Du behåller kontrollen.** Under _Anpassa, Säkerhet, Anslutna appar_ finns varje anslutning med sina grupper och sitt slutdatum; **Återkalla** avslutar den direkt. Anslutning, nekande, återkallelse och en token som använts två gånger finns i säkerhetsloggen.
- **Inget litas på i förväg.** Vilken applikation som helst får registrera sig själv, så sidan visar namnet den valt och adressen den återvänder till och säger att obop inte kan kontrollera vem som driver den. Återvändningsadresser måste vara `https` (eller `http` på `localhost`, `127.0.0.1` eller `[::1]`), registreringar är begränsade och oanvända löper ut.

Protokoll, för klientutvecklare: metadata enligt RFC 9728 och RFC 8414, dynamisk klientregistrering (RFC 7591, publika klienter), auktoriseringskod med PKCE **S256** (obligatoriskt), parametern `resource` (RFC 8707, den måste vara instansens `/mcp`-adress), rotation av förnyelsetokens med upptäckt av återanvändning, återkallelse (RFC 7009) och parametern `iss` i omdirigeringen (RFC 9207). Slutpunkter: `/oauth/register`, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke`. En loopback-omdirigering (`http://127.0.0.1:PORT/...`) får använda vilken port som helst (RFC 8252); alla andra måste stämma exakt. En instans bakom en proxy behöver `OBOP_PUBLIC_URL` så att adresserna i dessa dokument är de publika. Vi byggde detta efter specifikationen och testade det med vår egen klient i automatiska tester; vi har **ännu inte anslutit** de riktiga Claude-apparna.

### Verktyg

`tools/list` visar ett verktyg per kommando som token får använda, med namnet `<model>__<command>`: `party__search`, `lead__create`, `invoice__post`. Varje verktyg har ett JSON Schema `inputSchema` som genereras från modellen: pengar- och talfält är heltal, datum är `format: date`, urvalsfält är en `enum` av sina alternativ, ett referensfält är en sträng som anger sin målmodell, och obligatoriska fält finns i `required`. `list` och `search` tar `filter`, `order`, `limit` och `offset`; `get`, `delete` och åtgärder tar ett `id`. Annotationer markerar läsningar (`readOnlyHint`) och borttagning (`destructiveHint`). Ett extra skrivskyddat verktyg, `obop__meta`, returnerar det som `GET /api/meta` returnerar: börja där. Ett verktyg som token inte får använda listas inte och besvaras som ett verktyg som inte finns.

Resultaten har `structuredContent` och samma JSON som text:

```json
{ "status": "ok", "value": [{ "id": "...", "name": "NAME_1" }], "denied": ["party.tax_id"] }
```

- Ett steg som kräver en människa (varje borttagning, bokföring av en faktura) körs inte för en agent. Resultatet är ett vanligt resultat med `"status": "approval_required"`, ett `plan_id` och en uppmaning att fråga en människa, som godkänner det i back office eller med `POST /api/approve/<plan_id>`. Se [Godkännandeflödet](/docs/agents/approvals/).
- Ett avvisat indata (ett felaktigt värde, ett saknat fält) är ett verktygsfel: `isError: true` och serverns meddelande i `structuredContent.error`.
- `initialize` returnerar också `instructions`, en kort text som talar om för modellen hur instansen används.

`tools/list` delas upp i sidor med en `cursor` bara när en token ser fler än 200 verktyg. Listan meddelar inte ändringar (`listChanged` är `false`): starta en ny session efter att du har installerat en modul.

### Kontrollera med curl

```bash
export OBOP_URL=https://YOUR-INSTANCE
export OBOP_TOKEN=obop_...
curl -si -X POST "$OBOP_URL/mcp" \
  -H "Authorization: Bearer $OBOP_TOKEN" \
  -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}'
```

Kopiera headern `mcp-session-id` från svaret, skicka sedan `{"jsonrpc":"2.0","method":"notifications/initialized"}` (svar `202`) och därefter `{"jsonrpc":"2.0","id":2,"method":"tools/list"}`, båda med `-H "Mcp-Session-Id: <id>"`.

### Konfiguration av fjärrklienter

Byt ut `https://YOUR-INSTANCE/mcp` och förvara token i en miljövariabel eller ett hemlighetsvalv. Formaten nedan hämtades från varje klients dokumentation 2026-10-17. Vi har kört obop bara mot kommandoradsklienten MCP Inspector (`vp dlx @modelcontextprotocol/inspector --cli URL --transport http --header "Authorization: Bearer ..." --method tools/list`) och våra egna Playwright-tester. Vi startade inte de andra klienterna, så varje kodavsnitt är **dokumenterat, inte prövat**.

#### Claude Code

```bash
claude mcp add --transport http obop https://YOUR-INSTANCE/mcp \
  --header "Authorization: Bearer $OBOP_TOKEN"
```

Lägg till `--scope project` för att skriva en delad `.mcp.json` eller `--scope user` för att använda den i alla projekt. I en delad fil läser du token från miljön så att den inte checkas in:

```json
{
  "mcpServers": {
    "obop": {
      "type": "http",
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${OBOP_TOKEN}" }
    }
  }
}
```

Om servern avvisar headern rapporterar Claude Code en misslyckad anslutning i stället för att starta OAuth. Kontrollera med `/mcp`.

#### Claude Desktop och claude.ai

Apparna lägger till fjärrservrar som _custom connectors_ (Settings, Connectors, Add custom connector) och autentiserar dem med OAuth. Ange `https://YOUR-INSTANCE/mcp` och följ inloggningen i webbläsaren som beskrivs ovan; ingen token klistras in. För claude.ai måste instansen nås från internet över `https`, eftersom Anthropics servrar gör förfrågningarna. Stdio-uppsättningen [nedan](#claude-desktop-stdio) fungerar i Claude Desktop med en lokal binär. En brygga som communitypaketet `mcp-remote` kan lägga till en header till en fjärrserver för klienter som bara talar stdio; vi har inte testat den med obop.

#### Cursor

`.cursor/mcp.json` (projekt) eller `~/.cursor/mcp.json` (alla projekt):

```json
{
  "mcpServers": {
    "obop": {
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${env:OBOP_TOKEN}" }
    }
  }
}
```

#### VS Code

`.vscode/mcp.json`. Nyckeln på översta nivån är `servers`, och en input frågar efter token så att den inte skrivs till disk:

```json
{
  "inputs": [
    {
      "type": "promptString",
      "id": "obop-token",
      "description": "obop agent token",
      "password": true
    }
  ],
  "servers": {
    "obop": {
      "type": "http",
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${input:obop-token}" }
    }
  }
}
```

VS Codes dokumentation visade inget fullständigt exempel med både headers och en input, så det här kodavsnittet är sammansatt från dess fälttabeller.

#### Windsurf

Windsurfs dokumentation finns nu på Devin Desktops webbplats. Den anger `~/.config/devin/mcp_config.json` (macOS och Linux) och `%APPDATA%\devin\mcp_config.json` (Windows); äldre Windsurf-installationer använde `~/.codeium/windsurf/mcp_config.json`. Kontrollera vilken din version läser. Fjärrservrar använder `serverUrl` (eller `url`) och `headers`, och `${env:NAME}` expanderas:

```json
{
  "mcpServers": {
    "obop": {
      "serverUrl": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${env:OBOP_TOKEN}" }
    }
  }
}
```

#### Codex CLI

I `~/.codex/config.toml`. `bearer_token_env_var` anger miljövariabeln som innehåller token; Codex skickar den som en `Authorization`-bärartoken:

```toml
[mcp_servers.obop]
url = "https://YOUR-INSTANCE/mcp"
bearer_token_env_var = "OBOP_TOKEN"
```

`codex mcp add` dokumenterar `--url` bara med OAuth-alternativ, så använd filen för en bärartoken.

#### Gemini CLI

Dokumentationen vi läste säger att Gemini CLI ersattes av Antigravity CLI 2026-06-18; kontrollera sidan för det verktyg du använder. Formatet för Gemini CLI är `httpUrl` för Streamable HTTP (`url` gäller den äldre SSE-transporten), i `~/.gemini/settings.json`:

```json
{
  "mcpServers": {
    "obop": {
      "httpUrl": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer your-agent-token" }
    }
  }
}
```

eller `gemini mcp add --transport http --header "Authorization: Bearer $OBOP_TOKEN" obop https://YOUR-INSTANCE/mcp`. Vi kunde inte bekräfta att den här filen expanderar miljövariabler i `headers`, så token är utskriven här.

### Upptäckt

En instans publicerar dessa utan token:

| URL                                       | Vad                                                                                                                                                                           |
| ----------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/.well-known/oauth-protected-resource`   | Metadata om OAuth-skyddad resurs (RFC 9728) för `/mcp`, som nämner auktoriseringsservern. Finns också på `/.well-known/oauth-protected-resource/mcp`, dit `401`-svaret pekar. |
| `/.well-known/oauth-authorization-server` | Metadata om OAuth-auktoriseringsservern (RFC 8414): slutpunkterna, bara PKCE `S256`, bara publika klienter.                                                                   |
| `/.well-known/mcp/server-card.json`       | MCP-serverkort: namn, version, slutpunkten `streamable-http`, protokollversioner, headern `Authorization`. Finns också på `/mcp/server-card`.                                 |
| `/.well-known/ai-catalog.json`            | En AI-katalog med en post som pekar på serverkortet.                                                                                                                          |
| `/api/openapi.json`, `/api/meta`          | OpenAPI-dokumentet har ett `x-mcp`-fält med slutpunkt, transport och protokollversioner; `GET /api/meta` har en `Link`-header med `rel="mcp"`.                                |

Serverkortet är en utökning av MCP-specifikationen som fortfarande utvecklas (SEP-2127, förvarad i ett inkubationsarkiv), så dess form kan ändras; filen säger det i sin `$comment`. Det innehåller också de enkla fälten `serverInfo`, `transport`, `endpoint` och `capabilities` från ett tidigare utkast, för skannrar som letar efter dem.

## MCP över stdio (lokal binärfil)

Använd detta när klienten bara kan starta ett lokalt program, eller när du kör obop på samma maskin. Fjärrservern ovan behöver inget av detta.

### Så fungerar det

`obop mcp` är en MCP-server som talar JSON-RPC över standardin- och standardutmatning. En MCP-klient startar den som en underprocess. Den:

- öppnar instansens databas själv, så den måste köras på en maskin som når databasen (SQLite-fil eller PostgreSQL-URL);
- identifierar agenten med en agenttoken;
- listar samma verktyg, med samma scheman, som fjärrservern (båda transporterna kör en och samma kodväg): ett verktyg per kommando som token får använda, med namnet `<model>__<command>`. Det dubbla understrecket står där REST-sökvägen har ett snedstreck;
- besvarar `tools/call` genom samma dispatcher som REST, så maskering och godkännanden gäller. Maskeringstoken är stabila så länge processen körs.

### Innan du börjar

1. En databas för en körande instans, och binärfilen `obop` på maskinen som kör MCP-klienten (eller använd dess absoluta sökväg).
2. En [agenttoken](/docs/agents/tokens/) med de scope du vill ha.
3. Databasadressen, som `OBOP_DATABASE_URL` eller `--db`. Standard är `sqlite://obop.db` i den aktuella katalogen, så ange en absolut sökväg: `sqlite:///home/me/obop.db`.

Kommandot som varje klient kör är:

```bash
obop mcp --db sqlite:///home/me/obop.db --token obop_...
```

Det läser `OBOP_TOKEN` och `OBOP_DATABASE_URL` från miljön om flaggorna saknas. Föredra miljön: då hamnar token inte i processlistor.

Kontrollera det för hand först. Det här skickar en `initialize`- och en `tools/list`-förfrågan och skriver ut svaren:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

Servern svarar med den revision klienten ber om om den är någon av `2025-11-25`, `2025-06-18` och `2025-03-26`, och stöder `initialize`, `ping`, `tools/list` och `tools/call`. Verktygens argument är samma JSON som i [REST-bodyn](/docs/reference/rest-api/).

Kodavsnitten nedan använder `/usr/local/bin/obop` och en platshållartoken. Byt ut båda. Formaten kontrollerades mot varje klients dokumentation 2026-10-16; där något inte gick att bekräfta är det markerat.

### Claude Desktop (stdio)

Öppna _Settings_, sedan _Developer_, sedan _Edit Config_. Filen är `~/Library/Application Support/Claude/claude_desktop_config.json` på macOS och `%APPDATA%\Claude\claude_desktop_config.json` på Windows. Lägg till:

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "obop_your_agent_token",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

Avsluta och starta om Claude Desktop. Claude frågar efter ditt godkännande innan den använder ett verktyg.

### Claude Code (stdio)

Använd kommandoraden. Allt efter `--` är serverkommandot:

```bash
claude mcp add --transport stdio \
  --env OBOP_TOKEN=obop_your_agent_token \
  --env OBOP_DATABASE_URL=sqlite:///home/me/obop.db \
  obop -- /usr/local/bin/obop mcp
```

Lägg minst ett annat alternativ (här `--transport stdio`) mellan den sista `--env` och servernamnet. Lägg till `--scope project` för att i stället skriva en delad `.mcp.json`, eller `--scope user` för att använda den i alla projekt. En projektfil ser ut så här; formen `${...}` läser värdet från din miljö så att token inte checkas in:

```json
{
  "mcpServers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///home/me/obop.db"
      }
    }
  }
}
```

Kontrollera det med `claude mcp get obop` eller `/mcp` i Claude Code.

### Cursor (stdio)

Skapa `.cursor/mcp.json` i ett projekt, eller `~/.cursor/mcp.json` för alla projekt:

```json
{
  "mcpServers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${env:OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

Cursors egna exempel utelämnar `type` för stdio-servrar; dess fälttabell listar det. `${env:NAME}` läser från den miljö som Cursor startades i.

### VS Code (stdio)

Skapa `.vscode/mcp.json`. VS Codes nyckel på översta nivån är `servers`, inte `mcpServers`, och den kan fråga efter token så att den aldrig skrivs till disk:

```json
{
  "inputs": [
    {
      "type": "promptString",
      "id": "obop-token",
      "description": "obop agent token",
      "password": true
    }
  ],
  "servers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${input:obop-token}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

VS Codes guidade kommando _MCP: Add Server_ kan i stället föreslå en portabel `.mcp.json` i formatet `mcpServers`; enligt dokumentationen accepteras båda.

### Windsurf (stdio)

Windsurfs dokumentation har flyttat till Devin Desktops webbplats och beskriver filen som `mcp_config.json`. Vi kunde inte bekräfta den nuvarande filplatsen för Windsurf själv. Windsurf har tidigare använt `~/.codeium/windsurf/mcp_config.json`; sidan för Devin Desktop anger `~/.config/devin/mcp_config.json` (macOS och Linux) och `%APPDATA%\devin\mcp_config.json` (Windows). Kontrollera platsen i inställningarna för din version. Formatet är samma `mcpServers`-struktur:

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${env:OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

### Codex CLI (stdio)

Lägg till en tabell i `~/.codex/config.toml` (eller `.codex/config.toml` i ett betrott projekt), eller kör `codex mcp add`:

```toml
[mcp_servers.obop]
command = "/usr/local/bin/obop"
args = ["mcp"]
env_vars = ["OBOP_TOKEN"]

[mcp_servers.obop.env]
OBOP_DATABASE_URL = "sqlite:///home/me/obop.db"
```

`env_vars` vidarebefordrar `OBOP_TOKEN` från ditt skal i stället för att skriva in den i filen.

### Gemini CLI (stdio)

Lägg till i `~/.gemini/settings.json`:

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "$OBOP_TOKEN",
        "OBOP_DATABASE_URL": "sqlite:///home/me/obop.db"
      }
    }
  }
}
```

Gemini CLI:s dokumentation säger att det ersattes av ett annat verktyg under 2026, så kontrollera den sidan för din version. Vi har inte kört obop mot det.

### Valfri annan MCP-klient

obop är en vanlig stdio-server. Hur klientens fil än ser ut behöver den ett `command` (`obop`), `args` (`["mcp"]`) och två miljövariabler. Om klienten bara stöder fjärrservrar använder du fjärrservern ovan.

## Vad agenten ser

När du har anslutit ber du agenten lista sina verktyg. Du ska bara se de kommandon som dess scope tillåter, med en anmärkning på dem som kräver godkännande. Prova en läsning först, till exempel _"List the three newest leads"_, och läs sedan [Godkännandeflödet](/docs/agents/approvals/).

Om klienten inte visar några verktyg eller visar ett fel, gå till [Felsökning](/docs/agents/troubleshooting/).

Varje modell som agenten får skapa har också ett verktyg `<model>__bulk_create`: upp till 1 000 rader i ett anrop (`rows` eller `csv`-text), som standard allt eller inget, `mode: "partial"` behåller de rätta raderna och listar de felaktiga, `dry_run: true` kontrollerar först och en `idempotency_key` gör ett nytt försök säkert. Maskning, omfång och godkännande är desamma som för `create`.

En agent vars ägare är administratör ser också verktygen `user_account` (`user_account__list`, `__get`, `__add`, `__set_groups`, `__offboard`, `__disable`, `__enable`). De som ändrar ett konto svarar alltid `approval_required`: en administratör godkänner planen, och den körs som administratörens aktuella konto. `__add` kräver ett lösenord som en människa valt; servern ger aldrig en agent ett genererat lösenord.

## Verifierat och inte verifierat

| Punkt                                                                                                        | Status                                                                                  |
| ------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| `obop mcp`-handskakning, `tools/list`, `tools/call`, godkännanderesultat                                     | Körda mot en riktig lokal instans.                                                      |
| Fjärr-`/mcp`: handskakning, verktyg, maskering, godkännandeflöde, sessioner, 401, ursprungskontroll, gränser | Automatiska tester (Rust över HTTP, Playwright) mot en riktig instans.                  |
| Fjärr-`/mcp` med en riktig MCP-klient                                                                        | Kommandoradsklienten MCP Inspector 2.11.0: `initialize` och `tools/list`, `tools/call`. |
| Officiell MCP-konformitetssvit                                                                               | Inte meningsfull: den kräver en server utan autentisering och egna fixtureverktyg.      |
| Fjärrkonfiguration för Claude Code, Cursor, VS Code, Windsurf, Codex CLI, Gemini CLI                         | Hämtad från varje klients dokumentation; klienterna startades inte.                     |
| Konfigurationsformat för stdio                                                                               | Hämtade från varje klients dokumentation. Vi startade inte alla klienter.               |
| Windsurfs filplats                                                                                           | Obekräftad, se ovan.                                                                    |
| Inloggning i webbläsaren (OAuth): registrering, PKCE, samtycke, tokens, återanvändning, återkallelse         | Automatiska tester (Rust och Playwright i en riktig webbläsare) med vår egen klient.    |
| Connectors i Claude Desktop och claude.ai, inloggning i Claude Code                                          | **Inte prövat** med de riktiga apparna; byggt efter specifikationen.                    |
