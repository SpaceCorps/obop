---
title: "Anslut via MCP"
description: "Anslut Claude Desktop, Claude Code, Cursor, VS Code, Windsurf, Codex eller Gemini CLI till obop med Model Context Protocol: exakt konfiguration för varje klient, med obop mcp över stdio."
lead: "obop levereras med en MCP-server. Varje kommando som token får använda blir ett MCP-verktyg. Den här sidan visar konfigurationen för de vanligaste klienterna."
---

## Så fungerar det

`obop mcp` är en MCP-server som talar JSON-RPC över standardin- och standardutmatning. En MCP-klient startar den som en underprocess. Den:

- öppnar instansens databas själv, så den måste köras på en maskin som når databasen (SQLite-fil eller PostgreSQL-URL);
- identifierar agenten med en agenttoken;
- listar ett verktyg per kommando som token får använda, med namnet `<model>__<command>`: `lead__search`, `lead__create`, `invoice__post`. Det dubbla understrecket står där REST-sökvägen har ett snedstreck;
- besvarar `tools/call` genom samma dispatcher som REST, så maskering och godkännanden gäller.

> **Hostade instanser.** Det finns ännu ingen HTTP-transport för fjärr-MCP, och `obop mcp` behöver databasen. Du kan inte rikta en MCP-klient på skrivbordet mot en hostad instans i dag. För hostade instanser använder du [REST](/docs/agents/rest/). MCP över HTTP med OAuth är planerat.

## Innan du börjar

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
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

Servern rapporterar protokollversion `2025-06-18` och stöder `initialize`, `ping`, `tools/list` och `tools/call`. Verktygens indatascheman är i dag bara `{"type":"object"}`, så en klient kan inte validera argumenten; argumenten är samma JSON som i [REST-bodyn](/docs/reference/rest-api/).

Kodavsnitten nedan använder `/usr/local/bin/obop` och en platshållartoken. Byt ut båda. Formaten kontrollerades mot varje klients dokumentation 2026-10-16; där något inte gick att bekräfta är det markerat.

## Claude Desktop

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

## Claude Code

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

## Cursor

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

## VS Code

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

## Windsurf

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

## Codex CLI

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

## Gemini CLI

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

## Valfri annan MCP-klient

obop är en vanlig stdio-server. Hur klientens fil än ser ut behöver den ett `command` (`obop`), `args` (`["mcp"]`) och två miljövariabler. Om klienten bara stöder fjärrservrar kan den inte ansluta ännu.

## Vad agenten ser

När du har anslutit ber du agenten lista sina verktyg. Du ska bara se de kommandon som dess scope tillåter, med tillägget "(agents propose; a human approves)" på dem som kräver godkännande. Prova en läsning först, till exempel _"List the three newest leads"_, och läs sedan om [godkännandeflödet](/docs/agents/approvals/).

Om klienten inte visar några verktyg eller visar ett fel, gå till [Felsökning](/docs/agents/troubleshooting/).

## Verifierat och inte verifierat

| Punkt                                                                                    | Status                                                                    |
| ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `obop mcp`-handskakning, `tools/list`, `tools/call`, godkännanderesultat                 | Körda mot en riktig lokal instans.                                        |
| Konfigurationsformat för Claude Desktop, Claude Code, Cursor, VS Code, Codex, Gemini CLI | Hämtade från varje klients dokumentation. Vi startade inte alla klienter. |
| Windsurfs filplats                                                                       | Obekräftad, se ovan.                                                      |
