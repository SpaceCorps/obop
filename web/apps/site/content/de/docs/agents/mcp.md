---
title: "Über MCP verbinden"
description: "Verbinden Sie Claude Code, Cursor, VS Code, Windsurf, Codex oder Gemini CLI per URL mit einer obop-Instanz über den entfernten MCP-Server (Streamable HTTP, Bearer-Token), oder starten Sie obop mcp über stdio. Die genaue Konfiguration für jeden Client."
lead: "Jede obop-Instanz hat einen MCP-Server unter /mcp. Richten Sie einen Client mit einem Agent-Token auf die URL, und jeder Befehl, den das Token nutzen darf, wird zu einem MCP-Tool: Eine lokale Binärdatei ist nicht nötig."
---

## Entfernter MCP: per URL verbinden

Eine Instanz stellt das [Model Context Protocol](https://modelcontextprotocol.io/) über **Streamable HTTP** unter `https://YOUR-INSTANCE/mcp` bereit. Sowohl eine gehostete Instanz (zum Beispiel auf Sliplane) als auch ein lokales `obop serve` haben ihn; es muss nichts eingeschaltet werden.

- **Authentifizierung:** eine Anmeldung im Browser (OAuth, siehe unten) für Claude Desktop, claude.ai und Claude Code, oder `Authorization: Bearer <token>` bei jeder Anfrage mit einem Agent-Token. In beiden Fällen wird das Token wie bei REST geprüft. Fehlt es oder ist es ungültig, antwortet der Server mit `401` und `WWW-Authenticate: Bearer resource_metadata="https://YOUR-INSTANCE/.well-known/oauth-protected-resource/mcp"`, wie es die MCP-Autorisierungsspezifikation verlangt; dieses Dokument nennt den Autorisierungsserver der Instanz (`/.well-known/oauth-authorization-server`). Ein bei der Anmeldung ausgestelltes Token gilt nur für `/mcp`, nicht für `/api`.
- **Protokollversionen:** `2025-11-25`, `2025-06-18` und `2025-03-26`, ausgehandelt in `initialize`. Die neueste Version, `2026-07-28`, ist zustandslos und funktioniert anders; obop setzt sie noch nicht um. Ein Client, der beide spricht, fällt auf `initialize` zurück, wenn er ein schlichtes `400` sieht; das sendet obop bei einem nicht unterstützten Versions-Header.
- **Sitzungen:** `initialize` liefert eine `Mcp-Session-Id`; senden Sie sie bei jeder weiteren Anfrage mit. Die Sitzung hält den Maskierungs-Tresor, sodass ein Maskierungs-Token wie `NAME_1` in jedem Aufruf der Sitzung dasselbe bedeutet und Sie es zurücksenden können. Eine andere Sitzung beginnt mit einem neuen Tresor. `DELETE /mcp` beendet eine Sitzung; eine unbekannte oder abgelaufene Sitzung beantwortet der Server mit `404`, und der Client beginnt erneut mit `initialize`. Eine Sitzung gehört dem Token, das sie geöffnet hat, und läuft nach zwei Stunden ohne Anfrage ab.
- **Antworten sind schlichtes JSON.** Es gibt keinen SSE-Stream vom Server zum Client (`GET /mcp` antwortet mit `405`), und JSON-RPC-Batches werden nur in `2025-03-26` akzeptiert, der einzigen Version, die sie kennt.
- **Grenzen:** Anfragekörper bis zu 1 MiB (`413`), 600 Anfragen pro Minute und Token (`429` mit `Retry-After`, einstellbar mit `OBOP_MCP_RATE_PER_MINUTE`), 25 offene Sitzungen pro Token.
- **Browser:** Eine Anfrage mit einem `Origin`-Header wird mit `403` abgelehnt, es sei denn, der Origin ist die eigene öffentliche URL der Instanz (`OBOP_PUBLIC_URL`), derselbe Origin auf einem Loopback-Host oder in `OBOP_ALLOWED_ORIGINS` aufgeführt (kommagetrennt). Das schützt lokale Instanzen vor DNS-Rebinding. Clients, die keine Browser sind, senden keinen `Origin`. Setzen Sie hinter einem Proxy `OBOP_PUBLIC_URL` (zum Beispiel `https://acme.example.com`), damit die Discovery-Dokumente die öffentliche Adresse nennen.

### Mit einer Browser-Anmeldung verbinden (OAuth)

Claude Desktop, claude.ai und Claude Code können sich ohne eingefügtes Token mit einer Instanz verbinden. Fügen Sie in der App einen Custom Connector mit der Adresse `https://YOUR-INSTANCE/mcp` hinzu (Claude Code: `claude mcp add --transport http obop https://YOUR-INSTANCE/mcp`, dann `/mcp` und die Authentifizierung wählen). Die App findet die Anmeldung selbst und öffnet im Browser eine Seite **Ihrer Instanz**. Dort:

1. melden Sie sich mit Ihrem obop-Konto an und geben, falls das Konto es hat, Ihren Zwei-Faktor-Code ein;
2. sehen Sie, welche Anwendung anfragt und zu welcher Adresse der Browser zurückkehrt;
3. kreuzen Sie die Berechtigungsgruppen an, die Sie weitergeben, wählen, wie lange der Zugriff gilt (1, 7, 30 oder 90 Tage), und drücken **Erlauben**.

Was das bedeutet:

- **Die App kann Sie nur einschränken.** Angeboten werden die Gruppen, die Sie selbst haben; die App handelt als Sie mit den angekreuzten, nie mit mehr ([Agent-Tokens](/docs/agents/tokens/) funktionieren genauso). Ihre Aktionen werden als Aktionen Ihres Agenten aufgezeichnet.
- **Freigaben brauchen weiterhin eine Person.** Ein Schritt, der eine Freigabe braucht, wartet in obop auf einen Menschen; die App kann ihre eigenen Vorschläge nicht freigeben.
- **Kurzlebige Tokens.** Die App erhält ein Zugriffs-Token, das eine Stunde gilt, und ein Refresh-Token, das einmal verwendet und jedes Mal ersetzt wird. Wird ein Refresh-Token je zweimal verwendet, endet die ganze Verbindung. Ein Zugriffs-Token aus dieser Anmeldung gilt nur für `/mcp`.
- **Sie behalten die Kontrolle.** Unter _Anpassen, Sicherheit, Verbundene Apps_ steht jede Verbindung mit ihren Gruppen und ihrem Ende; **Widerrufen** beendet sie sofort. Verbinden, Ablehnen, Widerrufen und ein doppelt verwendetes Token stehen im Sicherheitsprotokoll.
- **Nichts wird vorab vertraut.** Jede Anwendung darf sich selbst registrieren; die Seite zeigt daher den gewählten Namen und die Rücksprungadresse und sagt, dass obop nicht prüfen kann, wer sie betreibt. Rücksprungadressen müssen `https` sein (oder `http` auf `localhost`, `127.0.0.1` oder `[::1]`), Registrierungen sind begrenzt, ungenutzte laufen ab.

Protokoll, für Client-Entwickler: Metadaten nach RFC 9728 und RFC 8414, dynamische Client-Registrierung (RFC 7591, öffentliche Clients), Authorization Code mit PKCE **S256** (Pflicht), der Parameter `resource` (RFC 8707, er muss die `/mcp`-Adresse der Instanz sein), Rotation der Refresh-Tokens mit Erkennung von Wiederverwendung, Widerruf (RFC 7009) und der Parameter `iss` bei der Weiterleitung (RFC 9207). Endpunkte: `/oauth/register`, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke`. Eine Loopback-Weiterleitung (`http://127.0.0.1:PORT/...`) darf jeden Port nutzen (RFC 8252); jede andere muss genau übereinstimmen. Eine Instanz hinter einem Proxy braucht `OBOP_PUBLIC_URL`, damit die Adressen in diesen Dokumenten die öffentlichen sind. Wir haben das nach der Spezifikation gebaut und mit einem eigenen Client in automatisierten Tests geprüft; die echten Claude-Apps haben wir damit **noch nicht verbunden**.

### Tools

`tools/list` zeigt pro Befehl, den das Token nutzen darf, ein Tool, benannt `<model>__<command>`: `party__search`, `lead__create`, `invoice__post`. Jedes hat ein aus dem Modell erzeugtes JSON-Schema `inputSchema`: Geld- und Zahlenfelder sind Ganzzahlen, Datumsfelder haben `format: date`, Auswahlfelder sind ein `enum` ihrer Optionen, ein Ref-Feld ist ein String, der sein Zielmodell nennt, und Pflichtfelder stehen in `required`. `list` und `search` nehmen `filter`, `order`, `limit` und `offset`; `get`, `delete` und Aktionen nehmen eine `id`. Annotationen kennzeichnen Lesezugriffe (`readOnlyHint`) und Löschen (`destructiveHint`). Ein zusätzliches schreibgeschütztes Tool, `obop__meta`, liefert, was `GET /api/meta` liefert: Beginnen Sie dort. Ein Tool, das das Token nicht nutzen darf, wird nicht aufgelistet und antwortet wie ein Tool, das nicht existiert.

Ergebnisse haben `structuredContent` und dasselbe JSON als Text:

```json
{ "status": "ok", "value": [{ "id": "...", "name": "NAME_1" }], "denied": ["party.tax_id"] }
```

- Ein Schritt, der einen Menschen braucht (jedes Löschen, das Buchen einer Rechnung), wird für einen Agenten nicht ausgeführt. Das Ergebnis ist ein normales mit `"status": "approval_required"`, einer `plan_id` und der Anweisung, einen Menschen zu fragen, der es im Backoffice oder mit `POST /api/approve/<plan_id>` freigibt. Siehe [Der Freigabeablauf](/docs/agents/approvals/).
- Eine abgelehnte Eingabe (ein ungültiger Wert, ein fehlendes Feld) ist ein Tool-Fehler: `isError: true` und die Meldung des Servers in `structuredContent.error`.
- `initialize` liefert außerdem `instructions`, einen kurzen Text, der dem Modell sagt, wie es die Instanz verwendet.

`tools/list` wird nur dann mit einem `cursor` seitenweise ausgeliefert, wenn ein Token mehr als 200 Tools sieht. Die Liste kündigt keine Änderungen an (`listChanged` ist `false`): Beginnen Sie nach der Installation eines Moduls eine neue Sitzung.

### Mit curl prüfen

```bash
export OBOP_URL=https://YOUR-INSTANCE
export OBOP_TOKEN=obop_...
curl -si -X POST "$OBOP_URL/mcp" \
  -H "Authorization: Bearer $OBOP_TOKEN" \
  -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}'
```

Kopieren Sie den Header `mcp-session-id` der Antwort, senden Sie dann `{"jsonrpc":"2.0","method":"notifications/initialized"}` (Antwort `202`) und danach `{"jsonrpc":"2.0","id":2,"method":"tools/list"}`, beides mit `-H "Mcp-Session-Id: <id>"`.

### Konfiguration entfernter Clients

Ersetzen Sie `https://YOUR-INSTANCE/mcp` und halten Sie das Token in einer Umgebungsvariablen oder einem Secret-Store. Die Formate unten wurden am 2026-10-17 der Dokumentation des jeweiligen Clients entnommen. Wir haben obop nur mit dem Kommandozeilen-Client des MCP Inspector (`vp dlx @modelcontextprotocol/inspector --cli URL --transport http --header "Authorization: Bearer ..." --method tools/list`) und mit unseren eigenen Playwright-Tests ausgeführt. Die anderen Clients haben wir nicht gestartet; jeder Ausschnitt ist daher **dokumentiert, nicht ausprobiert**.

#### Claude Code

```bash
claude mcp add --transport http obop https://YOUR-INSTANCE/mcp \
  --header "Authorization: Bearer $OBOP_TOKEN"
```

Mit `--scope project` schreiben Sie eine gemeinsame `.mcp.json`, mit `--scope user` nutzen Sie es in allen Projekten. Lesen Sie in einer gemeinsamen Datei das Token aus der Umgebung, damit es nicht eingecheckt wird:

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

Lehnt der Server den Header ab, meldet Claude Code eine fehlgeschlagene Verbindung, statt OAuth zu starten. Prüfen Sie es mit `/mcp`.

#### Claude Desktop und claude.ai

Die Apps fügen entfernte Server als _Custom Connectors_ hinzu (Settings, Connectors, Add custom connector) und authentifizieren sie mit OAuth. Geben Sie `https://YOUR-INSTANCE/mcp` ein und folgen Sie der oben beschriebenen Browser-Anmeldung; es wird kein Token eingefügt. Für claude.ai muss die Instanz per `https` aus dem Internet erreichbar sein, weil die Server von Anthropic die Anfragen stellen. Das stdio-Setup [unten](#claude-desktop-stdio) funktioniert in Claude Desktop mit einer lokalen Binärdatei. Eine Brücke wie das Community-Paket `mcp-remote` kann einem entfernten Server für Clients, die nur stdio sprechen, einen Header hinzufügen; wir haben sie nicht mit obop getestet.

#### Cursor

`.cursor/mcp.json` (Projekt) oder `~/.cursor/mcp.json` (alle Projekte):

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

`.vscode/mcp.json`. Der Schlüssel auf oberster Ebene heißt `servers`, und eine Eingabe fragt nach dem Token, sodass es nicht auf die Festplatte geschrieben wird:

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

Die Dokumentation von VS Code zeigte kein vollständiges Beispiel mit Headern und einer Eingabe; dieser Ausschnitt ist daher aus den Feldtabellen zusammengesetzt.

#### Windsurf

Die Dokumentation von Windsurf liegt jetzt auf der Devin-Desktop-Website. Sie nennt `~/.config/devin/mcp_config.json` (macOS und Linux) und `%APPDATA%\devin\mcp_config.json` (Windows); ältere Windsurf-Installationen verwendeten `~/.codeium/windsurf/mcp_config.json`. Prüfen Sie, welche Datei Ihre Version liest. Entfernte Server verwenden `serverUrl` (oder `url`) und `headers`, und `${env:NAME}` wird aufgelöst:

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

In `~/.codex/config.toml`. `bearer_token_env_var` nennt die Umgebungsvariable, die das Token enthält; Codex sendet es als Bearer-Token im Header `Authorization`:

```toml
[mcp_servers.obop]
url = "https://YOUR-INSTANCE/mcp"
bearer_token_env_var = "OBOP_TOKEN"
```

`codex mcp add` dokumentiert `--url` nur mit OAuth-Optionen; nutzen Sie daher für ein Bearer-Token die Datei.

#### Gemini CLI

Laut der Dokumentation, die wir gelesen haben, wurde Gemini CLI am 2026-06-18 durch Antigravity CLI ersetzt; prüfen Sie die Seite des Werkzeugs, das Sie verwenden. Das Format von Gemini CLI ist `httpUrl` für Streamable HTTP (`url` gilt für den älteren SSE-Transport), in `~/.gemini/settings.json`:

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

oder `gemini mcp add --transport http --header "Authorization: Bearer $OBOP_TOKEN" obop https://YOUR-INSTANCE/mcp`. Wir konnten nicht bestätigen, dass diese Datei Umgebungsvariablen in `headers` auflöst, deshalb steht das Token hier im Klartext.

### Discovery

Eine Instanz veröffentlicht Folgendes ohne Token:

| URL                                       | Was                                                                                                                                                                                          |
| ----------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/.well-known/oauth-protected-resource`   | Metadaten der OAuth-geschützten Ressource (RFC 9728) für `/mcp`; nennen den Autorisierungsserver. Auch unter `/.well-known/oauth-protected-resource/mcp`, worauf die `401`-Antwort verweist. |
| `/.well-known/oauth-authorization-server` | Metadaten des OAuth-Autorisierungsservers (RFC 8414): die Endpunkte, nur PKCE `S256`, nur öffentliche Clients.                                                                               |
| `/.well-known/mcp/server-card.json`       | MCP-Server-Card: Name, Version, der Endpunkt `streamable-http`, Protokollversionen, der Header `Authorization`. Auch unter `/mcp/server-card`.                                               |
| `/.well-known/ai-catalog.json`            | Ein AI Catalog mit einem Eintrag, der auf die Server-Card verweist.                                                                                                                          |
| `/api/openapi.json`, `/api/meta`          | Das OpenAPI-Dokument hat ein Feld `x-mcp` mit Endpunkt, Transport und Protokollversionen; `GET /api/meta` trägt einen `Link`-Header mit `rel="mcp"`.                                         |

Die Server-Card ist eine Erweiterung der MCP-Spezifikation, die noch entwickelt wird (SEP-2127, in einem Inkubations-Repository geführt); ihre Form kann sich daher ändern, was die Datei in ihrem `$comment` selbst sagt. Sie enthält außerdem die schlichten Felder `serverInfo`, `transport`, `endpoint` und `capabilities` aus einem früheren Entwurf, für Scanner, die danach suchen.

## MCP über stdio (lokale Binärdatei)

Nutzen Sie dies, wenn der Client nur ein lokales Programm starten kann oder wenn Sie obop auf demselben Rechner betreiben. Der entfernte Server oben braucht nichts davon.

### So funktioniert es

`obop mcp` ist ein MCP-Server, der JSON-RPC über Standardeingabe und Standardausgabe spricht. Ein MCP-Client startet ihn als Kindprozess. Er:

- öffnet die Datenbank der Instanz selbst und muss deshalb auf einem Rechner laufen, der die Datenbank erreicht (SQLite-Datei oder PostgreSQL-URL);
- identifiziert den Agenten über ein Agent-Token;
- listet dieselben Tools mit denselben Schemas auf wie der entfernte Server (beide Transporte nutzen einen Codepfad): ein Tool pro Befehl, den das Token nutzen darf, benannt `<model>__<command>`. Der doppelte Unterstrich steht dort, wo der REST-Pfad einen Schrägstrich hat;
- beantwortet `tools/call` über denselben Dispatcher wie REST, sodass Maskierung und Freigaben gelten. Die Maskierungs-Tokens bleiben stabil, solange der Prozess läuft.

### Bevor Sie beginnen

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
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

Der Server antwortet mit der Version, die der Client anfordert, sofern sie `2025-11-25`, `2025-06-18` oder `2025-03-26` ist, und unterstützt `initialize`, `ping`, `tools/list` und `tools/call`. Die Argumente eines Tools sind dasselbe JSON wie im [REST-Body](/docs/reference/rest-api/).

Die Ausschnitte unten verwenden `/usr/local/bin/obop` und ein Platzhalter-Token. Ersetzen Sie beides. Die Formate wurden am 2026-10-16 mit der Dokumentation des jeweiligen Clients abgeglichen; wo etwas nicht bestätigt werden konnte, ist es gekennzeichnet.

### Claude Desktop (stdio)

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

### Claude Code (stdio)

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

### Cursor (stdio)

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

### VS Code (stdio)

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

### Windsurf (stdio)

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

### Codex CLI (stdio)

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

### Gemini CLI (stdio)

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

### Jeder andere MCP-Client

obop ist ein einfacher stdio-Server. Wie die Datei des Clients auch aussieht, sie braucht einen `command` (`obop`), `args` (`["mcp"]`) und zwei Umgebungsvariablen. Unterstützt der Client nur entfernte Server, nutzen Sie den entfernten Server oben.

## Was der Agent sieht

Bitten Sie den Agenten nach dem Verbinden, seine Tools aufzulisten. Sie sollten nur die Befehle sehen, die seine Scopes erlauben, mit einem Hinweis bei denen, die eine Freigabe brauchen. Probieren Sie zuerst einen Lesezugriff, zum Beispiel _„List the three newest leads“_, und lesen Sie dann [Der Freigabeablauf](/docs/agents/approvals/).

Zeigt der Client keine Tools oder einen Fehler, gehen Sie zu [Fehlerbehebung](/docs/agents/troubleshooting/).

## Geprüft und nicht geprüft

| Punkt                                                                                                    | Status                                                                                          |
| -------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `obop mcp`-Handshake, `tools/list`, `tools/call`, Freigabeergebnis                                       | Gegen eine echte lokale Instanz ausgeführt.                                                     |
| Entferntes `/mcp`: Handshake, Tools, Maskierung, Freigabeablauf, Sitzungen, 401, Origin-Prüfung, Grenzen | Automatisierte Tests (Rust über HTTP, Playwright) gegen eine echte Instanz.                     |
| Entferntes `/mcp` mit einem echten MCP-Client                                                            | Kommandozeilen-Client des MCP Inspector 2.11.0: `initialize` und `tools/list`, `tools/call`.    |
| Offizielle MCP-Konformitätssuite                                                                         | Nicht aussagekräftig: Sie braucht einen Server ohne Authentifizierung und eigene Fixture-Tools. |
| Entfernte Konfiguration von Claude Code, Cursor, VS Code, Windsurf, Codex CLI, Gemini CLI                | Der Dokumentation des jeweiligen Clients entnommen; die Clients wurden nicht gestartet.         |
| Konfigurationsformate für stdio                                                                          | Der Dokumentation des jeweiligen Clients entnommen. Wir haben nicht jeden Client gestartet.     |
| Speicherort der Windsurf-Datei                                                                           | Unbestätigt, siehe oben.                                                                        |
| Anmeldung im Browser (OAuth): Registrierung, PKCE, Zustimmung, Tokens, Wiederverwendung, Widerruf        | Automatisierte Tests (Rust und Playwright in einem echten Browser) mit unserem eigenen Client.  |
| Connectors in Claude Desktop und claude.ai, Anmeldung in Claude Code                                     | **Nicht ausprobiert** mit den echten Apps; nach der Spezifikation gebaut.                       |
