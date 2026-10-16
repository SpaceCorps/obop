---
title: "Połączenie przez MCP"
description: "Połącz Claude Desktop, Claude Code, Cursor, VS Code, Windsurf, Codex lub Gemini CLI z obop za pomocą Model Context Protocol: dokładna konfiguracja dla każdego klienta, z użyciem obop mcp przez stdio."
lead: "obop zawiera serwer MCP. Każde polecenie dostępne dla tokenu staje się narzędziem MCP. Ta strona podaje konfigurację popularnych klientów."
---

## Jak to działa

`obop mcp` to serwer MCP, który komunikuje się przez JSON-RPC na standardowym wejściu i wyjściu. Klient MCP uruchamia go jako proces potomny. Serwer:

- sam otwiera bazę danych instancji, więc musi działać na maszynie, która ma do niej dostęp (plik SQLite lub adres URL PostgreSQL);
- identyfikuje agenta po tokenie agenta;
- udostępnia jedno narzędzie na każde polecenie, którego może użyć token, o nazwie `<model>__<command>`: `lead__search`, `lead__create`, `invoice__post`. Podwójny podkreślnik zastępuje ukośnik ze ścieżki REST;
- obsługuje `tools/call` przez ten sam dyspozytor co REST, więc obowiązuje maskowanie i zatwierdzanie.

> **Instancje hostowane.** Nie ma jeszcze zdalnego transportu MCP (HTTP), a `obop mcp` potrzebuje dostępu do bazy danych. Dziś nie można skierować desktopowego klienta MCP na instancję hostowaną. W instancjach hostowanych używaj [REST](/docs/agents/rest/). MCP przez HTTP z OAuth jest w planach.

## Zanim zaczniesz

1. Działająca baza danych instancji oraz plik wykonywalny `obop` na maszynie, na której działa klient MCP (albo jego ścieżka bezwzględna).
2. [Token agenta](/docs/agents/tokens/) z potrzebnymi zakresami.
3. Adres bazy danych jako `OBOP_DATABASE_URL` lub `--db`. Domyślnie jest to `sqlite://obop.db` w bieżącym katalogu, więc podaj ścieżkę bezwzględną: `sqlite:///home/me/obop.db`.

Polecenie, które uruchamia każdy klient, to:

```bash
obop mcp --db sqlite:///home/me/obop.db --token obop_...
```

Jeśli brakuje flag, `OBOP_TOKEN` i `OBOP_DATABASE_URL` są odczytywane ze środowiska. Lepiej użyć środowiska: token nie trafia wtedy na listę procesów.

Najpierw sprawdź to ręcznie. Poniższe polecenie wysyła żądania `initialize` i `tools/list` oraz wypisuje odpowiedzi:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

Serwer zgłasza wersję protokołu `2025-06-18` i obsługuje `initialize`, `ping`, `tools/list` oraz `tools/call`. Schematy wejściowe narzędzi mają dziś postać jedynie `{"type":"object"}`, więc klient nie może zwalidować argumentów; argumenty to ten sam JSON co w [treści żądania REST](/docs/reference/rest-api/).

Poniższe fragmenty używają `/usr/local/bin/obop` i przykładowego tokenu. Zastąp oba. Formaty sprawdziliśmy w dokumentacji każdego klienta 2026-10-16; tam, gdzie czegoś nie udało się potwierdzić, zaznaczyliśmy to.

## Claude Desktop

Otwórz _Settings_, następnie _Developer_, potem _Edit Config_. Plik to `~/Library/Application Support/Claude/claude_desktop_config.json` w macOS i `%APPDATA%\Claude\claude_desktop_config.json` w Windows. Dodaj:

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

Zamknij Claude Desktop i uruchom go ponownie. Claude prosi o Twoją zgodę, zanim użyje narzędzia.

## Claude Code

Użyj wiersza poleceń. Wszystko po `--` to polecenie serwera:

```bash
claude mcp add --transport stdio \
  --env OBOP_TOKEN=obop_your_agent_token \
  --env OBOP_DATABASE_URL=sqlite:///home/me/obop.db \
  obop -- /usr/local/bin/obop mcp
```

Między ostatnim `--env` a nazwą serwera umieść co najmniej jedną inną opcję (tutaj `--transport stdio`). Dodaj `--scope project`, aby zapisać współdzielony plik `.mcp.json`, albo `--scope user`, aby używać serwera we wszystkich projektach. Plik projektu wygląda tak; forma `${...}` odczytuje wartość ze środowiska, dzięki czemu token nie trafia do repozytorium:

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

Sprawdź to poleceniem `claude mcp get obop` lub `/mcp` w Claude Code.

## Cursor

Utwórz `.cursor/mcp.json` w projekcie albo `~/.cursor/mcp.json` dla wszystkich projektów:

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

Własne przykłady Cursora pomijają `type` dla serwerów stdio; tabela pól go wymienia. `${env:NAME}` odczytuje wartość ze środowiska, w którym uruchomiono Cursora.

## VS Code

Utwórz `.vscode/mcp.json`. Klucz najwyższego poziomu w VS Code to `servers`, a nie `mcpServers`, a edytor może zapytać o token, dzięki czemu nigdy nie jest on zapisywany na dysku:

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

Prowadzone polecenie VS Code _MCP: Add Server_ może zamiast tego zaproponować przenośny plik `.mcp.json` w formacie `mcpServers`; według dokumentacji oba formaty są akceptowane.

## Windsurf

Dokumentacja Windsurf przeniosła się na stronę Devin Desktop i opisuje plik jako `mcp_config.json`. Nie udało nam się potwierdzić aktualnej lokalizacji pliku dla samego Windsurf. Windsurf historycznie używał `~/.codeium/windsurf/mcp_config.json`; strona Devin Desktop podaje `~/.config/devin/mcp_config.json` (macOS i Linux) oraz `%APPDATA%\devin\mcp_config.json` (Windows). Sprawdź lokalizację w ustawieniach swojej wersji. Format jest ten sam, w postaci `mcpServers`:

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

Dodaj tabelę do `~/.codex/config.toml` (albo `.codex/config.toml` w zaufanym projekcie) lub uruchom `codex mcp add`:

```toml
[mcp_servers.obop]
command = "/usr/local/bin/obop"
args = ["mcp"]
env_vars = ["OBOP_TOKEN"]

[mcp_servers.obop.env]
OBOP_DATABASE_URL = "sqlite:///home/me/obop.db"
```

`env_vars` przekazuje `OBOP_TOKEN` z Twojej powłoki zamiast zapisywać go w pliku.

## Gemini CLI

Dodaj do `~/.gemini/settings.json`:

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

Dokumentacja Gemini CLI podaje, że w 2026 roku zastąpiło go inne narzędzie, więc sprawdź tę stronę dla swojej wersji. Nie uruchamialiśmy obop z tym klientem.

## Dowolny inny klient MCP

obop to zwykły serwer stdio. Niezależnie od wyglądu pliku klienta potrzebne są `command` (`obop`), `args` (`["mcp"]`) i dwie zmienne środowiskowe. Jeśli klient obsługuje tylko serwery zdalne, nie może się jeszcze połączyć.

## Co widzi agent

Po połączeniu poproś agenta o wypisanie jego narzędzi. Powinny być widoczne tylko polecenia dozwolone przez jego zakresy, a przy tych, które wymagają zatwierdzenia, dopisek „(agents propose; a human approves)”. Zacznij od odczytu, na przykład _„Wypisz trzy najnowsze leady”_, a potem zobacz [Pętlę zatwierdzania](/docs/agents/approvals/).

Jeśli klient nie pokazuje narzędzi albo zgłasza błąd, przejdź do [Rozwiązywania problemów](/docs/agents/troubleshooting/).

## Co sprawdzono, a czego nie

| Element                                                                              | Stan                                                                              |
| ------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------- |
| Uzgodnienie połączenia `obop mcp`, `tools/list`, `tools/call`, wynik zatwierdzania   | Uruchomione na prawdziwej lokalnej instancji.                                     |
| Formaty konfiguracji Claude Desktop, Claude Code, Cursor, VS Code, Codex, Gemini CLI | Przepisane z dokumentacji każdego klienta. Nie uruchamialiśmy każdego z klientów. |
| Lokalizacja pliku Windsurf                                                           | Niepotwierdzona, zobacz wyżej.                                                    |
