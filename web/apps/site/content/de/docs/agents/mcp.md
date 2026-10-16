---
title: "Über MCP verbinden"
description: "Verbinden Sie Claude Desktop, Claude Code, Cursor, VS Code, Windsurf, Codex oder Gemini CLI mit obop über das Model Context Protocol: die genaue Konfiguration für jeden Client, mit obop mcp über stdio."
lead: "obop liefert einen MCP-Server mit. Jeder Befehl, den das Token nutzen darf, wird zu einem MCP-Tool. Diese Seite zeigt die Konfiguration für die gängigen Clients."
---

## So funktioniert es

`obop mcp` ist ein MCP-Server, der JSON-RPC über Standardeingabe und Standardausgabe spricht. Ein MCP-Client startet ihn als Kindprozess. Er:

- öffnet die Datenbank der Instanz selbst und muss deshalb auf einem Rechner laufen, der die Datenbank erreicht (SQLite-Datei oder PostgreSQL-URL);
- identifiziert den Agenten über ein Agent-Token;
- listet pro Befehl, den dieses Token nutzen darf, ein Tool auf, benannt `<model>__<command>`: `lead__search`, `lead__create`, `invoice__post`. Der doppelte Unterstrich steht dort, wo der REST-Pfad einen Schrägstrich hat;
- beantwortet `tools/call` über denselben Dispatcher wie REST, sodass Maskierung und Freigaben gelten.

> **Gehostete Instanzen.** Es gibt noch keinen entfernten (HTTP-)MCP-Transport, und `obop mcp` braucht die Datenbank. Sie können einen MCP-Desktop-Client heute nicht auf eine gehostete Instanz richten. Für gehostete Instanzen nutzen Sie [REST](/docs/agents/rest/). MCP über HTTP mit OAuth ist geplant.

## Bevor Sie beginnen

1. Eine laufende Instanzdatenbank und die `obop`-Binärdatei auf dem Rechner, auf dem der MCP-Client läuft (oder deren absoluter Pfad).
2. Ein [Agent-Token](/docs/agents/tokens/) mit den gewünschten Scopes.
3. Die Datenbankadresse, als `OBOP_DATABASE_URL` oder `--db`. Der Standard ist `sqlite://obop.db` im aktuellen Verzeichnis, geben Sie also einen absoluten Pfad an: `sqlite:///home/me/obop.db`.

Der Befehl, den jeder Client ausführt, lautet:

```bash
obop mcp --db sqlite:///home/me/obop.db --token obop_...
```

Fehlen die Optionen, liest er `OBOP_TOKEN` und `OBOP_DATABASE_URL` aus der Umgebung. Bevorzugen Sie die Umgebung: So taucht das Token nicht in Prozesslisten auf.

Prüfen Sie es zuerst von Hand. Das sendet eine `initialize`- und eine `tools/list`-Anfrage und gibt die Antworten aus:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

Der Server meldet die Protokollversion `2025-06-18` und unterstützt `initialize`, `ping`, `tools/list` und `tools/call`. Die Eingabeschemas der Tools sind heute nur `{"type":"object"}`, ein Client kann Argumente also nicht validieren; die Argumente sind dasselbe JSON wie im [REST-Body](/docs/reference/rest-api/).

Die Beispiele unten verwenden `/usr/local/bin/obop` und ein Platzhalter-Token. Ersetzen Sie beides. Die Formate wurden am 2026-10-16 mit der Dokumentation des jeweiligen Clients abgeglichen; wo etwas nicht bestätigt werden konnte, ist es gekennzeichnet.

## Claude Desktop

Öffnen Sie _Settings_, dann _Developer_, dann _Edit Config_. Die Datei ist unter macOS `~/Library/Application Support/Claude/claude_desktop_config.json` und unter Windows `%APPDATA%\Claude\claude_desktop_config.json`. Fügen Sie hinzu:

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

Beenden Sie Claude Desktop und starten Sie es neu. Claude bittet um Ihre Zustimmung, bevor es ein Tool verwendet.

## Claude Code

Nutzen Sie die Kommandozeile. Alles nach `--` ist der Serverbefehl:

```bash
claude mcp add --transport stdio \
  --env OBOP_TOKEN=obop_your_agent_token \
  --env OBOP_DATABASE_URL=sqlite:///home/me/obop.db \
  obop -- /usr/local/bin/obop mcp
```

Setzen Sie mindestens eine weitere Option (hier `--transport stdio`) zwischen das letzte `--env` und den Servernamen. Mit `--scope project` schreiben Sie stattdessen eine gemeinsame `.mcp.json`, mit `--scope user` nutzen Sie es in allen Projekten. Eine Projektdatei sieht so aus; die Form `${...}` liest den Wert aus Ihrer Umgebung, sodass das Token nicht eingecheckt wird:

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

Prüfen Sie es mit `claude mcp get obop` oder `/mcp` in Claude Code.

## Cursor

Legen Sie `.cursor/mcp.json` in einem Projekt an, oder `~/.cursor/mcp.json` für alle Projekte:

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

In den eigenen Beispielen von Cursor fehlt `type` bei stdio-Servern; seine Feldtabelle führt es auf. `${env:NAME}` liest aus der Umgebung, in der Cursor gestartet wurde.

## VS Code

Legen Sie `.vscode/mcp.json` an. Der Schlüssel auf oberster Ebene heißt in VS Code `servers`, nicht `mcpServers`, und VS Code kann nach dem Token fragen, sodass es nie auf die Festplatte geschrieben wird:

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

Der geführte Befehl _MCP: Add Server_ von VS Code schlägt möglicherweise stattdessen eine portable `.mcp.json` im Format `mcpServers` vor; laut Dokumentation werden beide akzeptiert.

## Windsurf

Die Dokumentation von Windsurf ist auf die Devin-Desktop-Website umgezogen und beschreibt die Datei als `mcp_config.json`. Den aktuellen Speicherort für Windsurf selbst konnten wir nicht bestätigen. Windsurf verwendete früher `~/.codeium/windsurf/mcp_config.json`; die Devin-Desktop-Seite nennt `~/.config/devin/mcp_config.json` (macOS und Linux) und `%APPDATA%\devin\mcp_config.json` (Windows). Prüfen Sie den Speicherort in den Einstellungen Ihrer Version. Das Format ist dieselbe Struktur mit `mcpServers`:

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

Fügen Sie `~/.codex/config.toml` (oder `.codex/config.toml` in einem vertrauenswürdigen Projekt) eine Tabelle hinzu oder führen Sie `codex mcp add` aus:

```toml
[mcp_servers.obop]
command = "/usr/local/bin/obop"
args = ["mcp"]
env_vars = ["OBOP_TOKEN"]

[mcp_servers.obop.env]
OBOP_DATABASE_URL = "sqlite:///home/me/obop.db"
```

`env_vars` reicht `OBOP_TOKEN` aus Ihrer Shell weiter, statt es in die Datei zu schreiben.

## Gemini CLI

Ergänzen Sie `~/.gemini/settings.json`:

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

Laut der Dokumentation von Gemini CLI wurde es 2026 durch ein anderes Werkzeug ersetzt, prüfen Sie also die Seite für Ihre Version. Wir haben obop nicht damit ausgeführt.

## Jeder andere MCP-Client

obop ist ein einfacher stdio-Server. Wie die Datei des Clients auch aussieht, sie braucht einen `command` (`obop`), `args` (`["mcp"]`) und zwei Umgebungsvariablen. Unterstützt der Client nur entfernte Server, kann er sich noch nicht verbinden.

## Was der Agent sieht

Bitten Sie den Agenten nach dem Verbinden, seine Tools aufzulisten. Sie sollten nur die Befehle sehen, die seine Scopes erlauben, mit dem Zusatz „(agents propose; a human approves)“ bei denen, die eine Freigabe brauchen. Probieren Sie zuerst einen Lesezugriff, zum Beispiel _„List the three newest leads“_, und lesen Sie dann [Der Freigabeablauf](/docs/agents/approvals/).

Zeigt der Client keine Tools oder einen Fehler, gehen Sie zu [Fehlerbehebung](/docs/agents/troubleshooting/).

## Geprüft und nicht geprüft

| Punkt                                                                                     | Status                                                                                      |
| ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `obop mcp`-Handshake, `tools/list`, `tools/call`, Freigabeergebnis                        | Gegen eine echte lokale Instanz ausgeführt.                                                 |
| Konfigurationsformate für Claude Desktop, Claude Code, Cursor, VS Code, Codex, Gemini CLI | Der Dokumentation des jeweiligen Clients entnommen. Wir haben nicht jeden Client gestartet. |
| Speicherort der Windsurf-Datei                                                            | Unbestätigt, siehe oben.                                                                    |
