---
title: "Połączenie przez MCP"
description: "Połącz Claude Code, Cursor, VS Code, Windsurf, Codex lub Gemini CLI z instancją obop przez adres URL za pomocą zdalnego serwera MCP (Streamable HTTP, token okaziciela) albo uruchom obop mcp przez stdio. Dokładna konfiguracja dla każdego klienta."
lead: "Każda instancja obop ma serwer MCP pod adresem /mcp. Skieruj klienta na ten adres z tokenem agenta, a każde polecenie dostępne dla tokenu stanie się narzędziem MCP: lokalny plik wykonywalny nie jest potrzebny."
---

## Zdalny MCP: połączenie przez adres URL

Instancja udostępnia [Model Context Protocol](https://modelcontextprotocol.io/) przez **Streamable HTTP** pod adresem `https://YOUR-INSTANCE/mcp`. Mają go zarówno instancja hostowana (na przykład w Sliplane), jak i lokalne `obop serve`; niczego nie trzeba włączać.

- **Uwierzytelnianie:** nagłówek `Authorization: Bearer <token>` w każdym żądaniu, z [tokenem agenta](/docs/agents/tokens/) (lub tokenem sesji z logowania). Token jest sprawdzany dokładnie tak jak w REST, więc lista narzędzi, uprawnienia, maskowanie i zatwierdzanie są takie, jakie token ma przez [REST](/docs/agents/rest/).
- **Jeszcze bez OAuth.** obop nie ma serwera autoryzacji OAuth. Punkt końcowy odpowiada `401` z nagłówkiem `WWW-Authenticate: Bearer resource_metadata="https://YOUR-INSTANCE/.well-known/oauth-protected-resource"`, jak wymaga tego specyfikacja autoryzacji MCP, a ten dokument jasno mówi, że tokeny pochodzą z `POST /api/auth/login` i `POST /api/auth/agent-token`, i nie wskazuje żadnego serwera autoryzacji. Klienci, którzy mogą się łączyć tylko przez przepływ OAuth w przeglądarce (na przykład okno „custom connector” w aplikacjach Claude), **nie mogą się jeszcze połączyć**; użyj jednego z klientów poniżej, który przyjmuje nagłówek.
- **Wersje protokołu:** `2025-11-25`, `2025-06-18` i `2025-03-26`, uzgadniane w `initialize`. Najnowsza wersja, `2026-07-28`, jest bezstanowa i działa inaczej; obop jeszcze jej nie implementuje. Klient obsługujący obie wersje wraca do `initialize`, gdy zobaczy zwykłe `400`, które obop wysyła dla nieobsługiwanego nagłówka wersji.
- **Sesje:** `initialize` zwraca `Mcp-Session-Id`; wysyłaj go w każdym kolejnym żądaniu. Sesja przechowuje skarbiec maskowania, więc token maski taki jak `NAME_1` oznacza to samo w każdym wywołaniu sesji i możesz go odesłać. Inna sesja zaczyna nowy skarbiec. `DELETE /mcp` kończy sesję; nieznana lub wygasła sesja dostaje odpowiedź `404`, a klient zaczyna od nowa od `initialize`. Sesja należy do tokenu, który ją otworzył, i wygasa po dwóch godzinach bez żądania.
- **Odpowiedzi to zwykły JSON.** Nie ma strumienia SSE od serwera do klienta (`GET /mcp` odpowiada `405`), a paczki JSON-RPC są przyjmowane tylko w wersji `2025-03-26`, jedynej, która je ma.
- **Limity:** treść żądania do 1 MiB (`413`), 600 żądań na minutę na token (`429` z `Retry-After`, do ustawienia przez `OBOP_MCP_RATE_PER_MINUTE`), 25 otwartych sesji na token.
- **Przeglądarki:** żądanie z nagłówkiem `Origin` jest odrzucane z kodem `403`, chyba że to jest własny publiczny adres instancji (`OBOP_PUBLIC_URL`), ten sam origin na hoście pętli zwrotnej albo origin z listy w `OBOP_ALLOWED_ORIGINS` (rozdzielonej przecinkami). To chroni instancje lokalne przed DNS rebinding. Klienci niebędący przeglądarkami nie wysyłają `Origin`. Za serwerem proxy ustaw `OBOP_PUBLIC_URL` (na przykład `https://acme.example.com`), aby dokumenty wykrywania podawały adres publiczny.

### Narzędzia

`tools/list` pokazuje jedno narzędzie na każde polecenie, którego może użyć token, o nazwie `<model>__<command>`: `party__search`, `lead__create`, `invoice__post`. Każde ma `inputSchema` w JSON Schema wygenerowany z modelu: pola kwot i liczb to liczby całkowite, daty mają `format: date`, pola wyboru to `enum` swoich opcji, pole referencji to ciąg znaków wskazujący model docelowy, a pola wymagane są w `required`. `list` i `search` przyjmują `filter`, `order`, `limit` i `offset`; `get`, `delete` i akcje przyjmują `id`. Adnotacje oznaczają odczyty (`readOnlyHint`) i usuwanie (`destructiveHint`). Jedno dodatkowe narzędzie tylko do odczytu, `obop__meta`, zwraca to, co `GET /api/meta`: zacznij od niego. Narzędzie, którego token nie może użyć, nie jest wymienione i odpowiada jak narzędzie, które nie istnieje.

Wyniki zawierają `structuredContent` oraz ten sam JSON jako tekst:

```json
{ "status": "ok", "value": [{ "id": "...", "name": "NAME_1" }], "denied": ["party.tax_id"] }
```

- Krok wymagający człowieka (każde usunięcie, zaksięgowanie faktury) nie jest wykonywany dla agenta. Wynik jest zwykły, z `"status": "approval_required"`, `plan_id` i instrukcją, by poprosić człowieka, który zatwierdza go w back office lub przez `POST /api/approve/<plan_id>`. Zobacz [Pętlę zatwierdzania](/docs/agents/approvals/).
- Odrzucone dane wejściowe (zła wartość, brakujące pole) to błąd narzędzia: `isError: true` oraz komunikat serwera w `structuredContent.error`.
- `initialize` zwraca także `instructions`, krótki tekst, który mówi modelowi, jak korzystać z instancji.

`tools/list` jest dzielone na strony za pomocą `cursor` tylko wtedy, gdy token widzi więcej niż 200 narzędzi. Lista nie ogłasza zmian (`listChanged` ma wartość `false`): po zainstalowaniu modułu rozpocznij nową sesję.

### Sprawdzenie za pomocą curl

```bash
export OBOP_URL=https://YOUR-INSTANCE
export OBOP_TOKEN=obop_...
curl -si -X POST "$OBOP_URL/mcp" \
  -H "Authorization: Bearer $OBOP_TOKEN" \
  -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}'
```

Skopiuj nagłówek `mcp-session-id` z odpowiedzi, następnie wyślij `{"jsonrpc":"2.0","method":"notifications/initialized"}` (odpowiedź `202`), a potem `{"jsonrpc":"2.0","id":2,"method":"tools/list"}`, oba z `-H "Mcp-Session-Id: <id>"`.

### Konfiguracja zdalnych klientów

Zastąp `https://YOUR-INSTANCE/mcp` własnym adresem i trzymaj token w zmiennej środowiskowej lub w magazynie sekretów. Poniższe formaty przepisano z dokumentacji każdego klienta 2026-10-17. obop uruchamialiśmy tylko z klientem wiersza poleceń MCP Inspector (`vp dlx @modelcontextprotocol/inspector --cli URL --transport http --header "Authorization: Bearer ..." --method tools/list`) i z własnymi testami Playwright. Pozostałych klientów nie uruchamialiśmy, więc każdy fragment jest **udokumentowany, ale nie wypróbowany**.

#### Claude Code

```bash
claude mcp add --transport http obop https://YOUR-INSTANCE/mcp \
  --header "Authorization: Bearer $OBOP_TOKEN"
```

Dodaj `--scope project`, aby zapisać współdzielony plik `.mcp.json`, albo `--scope user`, aby używać serwera we wszystkich projektach. We współdzielonym pliku odczytuj token ze środowiska, aby nie trafił do repozytorium:

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

Jeśli serwer odrzuci nagłówek, Claude Code zgłasza nieudane połączenie zamiast uruchamiać OAuth. Sprawdź to poleceniem `/mcp`.

#### Claude Desktop i claude.ai

Aplikacje dodają zdalne serwery jako _custom connectors_ (Settings, Connectors, Add custom connector) i uwierzytelniają je przez OAuth. Statyczny nagłówek z tokenem okaziciela nie jest tam dostępny, a obop nie ma serwera OAuth, więc **hostowanej instancji obop nie można jeszcze dodać jako konektora**. Konfiguracja stdio [poniżej](#claude-desktop-stdio) działa w Claude Desktop z lokalnym plikiem wykonywalnym. Most taki jak pakiet społeczności `mcp-remote` może dodać nagłówek do zdalnego serwera dla klientów, którzy obsługują tylko stdio; nie testowaliśmy go z obop.

#### Cursor

`.cursor/mcp.json` (projekt) lub `~/.cursor/mcp.json` (wszystkie projekty):

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

`.vscode/mcp.json`. Klucz najwyższego poziomu to `servers`, a pole `inputs` pyta o token, dzięki czemu nie jest on zapisywany na dysku:

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

Dokumentacja VS Code nie pokazuje jednego kompletnego przykładu z nagłówkami i polem `inputs`, więc ten fragment złożono z tabel pól.

#### Windsurf

Dokumentacja Windsurf znajduje się teraz na stronie Devin Desktop. Podaje ona `~/.config/devin/mcp_config.json` (macOS i Linux) oraz `%APPDATA%\devin\mcp_config.json` (Windows); starsze instalacje Windsurf używały `~/.codeium/windsurf/mcp_config.json`. Sprawdź, który plik czyta Twoja wersja. Zdalne serwery używają `serverUrl` (lub `url`) i `headers`, a `${env:NAME}` jest rozwijane:

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

W `~/.codex/config.toml`. `bearer_token_env_var` wskazuje zmienną środowiskową z tokenem; Codex wysyła go jako token okaziciela w nagłówku `Authorization`:

```toml
[mcp_servers.obop]
url = "https://YOUR-INSTANCE/mcp"
bearer_token_env_var = "OBOP_TOKEN"
```

`codex mcp add` w dokumentacji ma `--url` tylko z opcjami OAuth, więc dla tokenu okaziciela użyj pliku.

#### Gemini CLI

Dokumentacja, którą czytaliśmy, podaje, że Gemini CLI zastąpiono Antigravity CLI 2026-06-18; sprawdź stronę narzędzia, którego używasz. Format Gemini CLI to `httpUrl` dla Streamable HTTP (`url` służy do starszego transportu SSE), w `~/.gemini/settings.json`:

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

lub `gemini mcp add --transport http --header "Authorization: Bearer $OBOP_TOKEN" obop https://YOUR-INSTANCE/mcp`. Nie udało nam się potwierdzić, że ten plik rozwija zmienne środowiskowe w `headers`, więc token jest tu wpisany wprost.

### Wykrywanie

Instancja publikuje poniższe zasoby bez tokenu:

| URL                                     | Co to jest                                                                                                                                                           |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/.well-known/oauth-protected-resource` | Metadane chronionego zasobu OAuth (RFC 9728) dla `/mcp`. Stwierdzają, że nie ma serwera autoryzacji. Dostępne także pod `/.well-known/oauth-protected-resource/mcp`. |
| `/.well-known/mcp/server-card.json`     | Karta serwera MCP: nazwa, wersja, punkt końcowy `streamable-http`, wersje protokołu, nagłówek `Authorization`. Dostępna także pod `/mcp/server-card`.                |
| `/.well-known/ai-catalog.json`          | Jednopozycyjny katalog AI (AI Catalog), który wskazuje kartę serwera.                                                                                                |
| `/api/openapi.json`, `/api/meta`        | Dokument OpenAPI ma pole `x-mcp` z punktem końcowym, transportem i wersjami protokołu; `GET /api/meta` zwraca nagłówek `Link` z `rel="mcp"`.                         |

Karta serwera jest rozszerzeniem specyfikacji MCP, które wciąż jest rozwijane (SEP-2127, w repozytorium inkubacyjnym), więc jej kształt może się zmienić; plik mówi o tym w polu `$comment`. Zawiera też zwykłe pola `serverInfo`, `transport`, `endpoint` i `capabilities` z wcześniejszego szkicu, dla skanerów, które ich szukają.

## MCP przez stdio (lokalny plik wykonywalny)

Użyj tego, gdy klient potrafi uruchomić tylko program lokalny albo gdy obop działa na tej samej maszynie. Zdalny serwer opisany wyżej nie wymaga żadnego z tych kroków.

### Jak to działa

`obop mcp` to serwer MCP, który komunikuje się przez JSON-RPC na standardowym wejściu i wyjściu. Klient MCP uruchamia go jako proces potomny. Serwer:

- sam otwiera bazę danych instancji, więc musi działać na maszynie, która ma do niej dostęp (plik SQLite lub adres URL PostgreSQL);
- identyfikuje agenta po tokenie agenta;
- wymienia te same narzędzia, z tymi samymi schematami, co serwer zdalny (oba transporty używają jednej ścieżki kodu): jedno narzędzie na każde polecenie, którego może użyć token, o nazwie `<model>__<command>`. Podwójny podkreślnik zastępuje ukośnik ze ścieżki REST;
- obsługuje `tools/call` przez ten sam dyspozytor co REST, więc obowiązuje maskowanie i zatwierdzanie. Tokeny masek są stałe, dopóki proces działa.

### Zanim zaczniesz

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
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

Serwer odpowiada wersją, o którą prosi klient, jeśli jest to jedna z `2025-11-25`, `2025-06-18` i `2025-03-26`, i obsługuje `initialize`, `ping`, `tools/list` oraz `tools/call`. Argumenty narzędzia to ten sam JSON co w [treści żądania REST](/docs/reference/rest-api/).

Poniższe fragmenty używają `/usr/local/bin/obop` i przykładowego tokenu. Zastąp oba. Formaty sprawdziliśmy w dokumentacji każdego klienta 2026-10-16; tam, gdzie czegoś nie udało się potwierdzić, zaznaczyliśmy to.

### Claude Desktop (stdio)

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

### Claude Code (stdio)

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

### Cursor (stdio)

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

### VS Code (stdio)

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

### Windsurf (stdio)

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

### Codex CLI (stdio)

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

### Gemini CLI (stdio)

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

### Dowolny inny klient MCP

obop to zwykły serwer stdio. Niezależnie od wyglądu pliku klienta potrzebne są `command` (`obop`), `args` (`["mcp"]`) i dwie zmienne środowiskowe. Jeśli klient obsługuje tylko serwery zdalne, użyj zdalnego serwera opisanego wyżej.

## Co widzi agent

Po połączeniu poproś agenta o wypisanie jego narzędzi. Powinny być widoczne tylko polecenia dozwolone przez jego zakresy, z dopiskiem przy tych, które wymagają zatwierdzenia. Zacznij od odczytu, na przykład _„Wypisz trzy najnowsze leady”_, a potem zobacz [Pętlę zatwierdzania](/docs/agents/approvals/).

Jeśli klient nie pokazuje narzędzi albo zgłasza błąd, przejdź do [Rozwiązywania problemów](/docs/agents/troubleshooting/).

## Co sprawdzono, a czego nie

| Element                                                                                                        | Stan                                                                                    |
| -------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Uzgodnienie połączenia `obop mcp`, `tools/list`, `tools/call`, wynik zatwierdzania                             | Uruchomione na prawdziwej lokalnej instancji.                                           |
| Zdalny `/mcp`: uzgodnienie, narzędzia, maskowanie, pętla zatwierdzania, sesje, 401, sprawdzanie origin, limity | Testy automatyczne (Rust przez HTTP, Playwright) na prawdziwej instancji.               |
| Zdalny `/mcp` z prawdziwym klientem MCP                                                                        | Klient wiersza poleceń MCP Inspector 2.11.0: `initialize` i `tools/list`, `tools/call`. |
| Oficjalny zestaw testów zgodności MCP                                                                          | Nie ma sensu: wymaga serwera bez uwierzytelniania i własnych narzędzi testowych.        |
| Zdalna konfiguracja Claude Code, Cursor, VS Code, Windsurf, Codex CLI, Gemini CLI                              | Przepisana z dokumentacji każdego klienta; klientów nie uruchamialiśmy.                 |
| Formaty konfiguracji stdio                                                                                     | Przepisane z dokumentacji każdego klienta. Nie uruchamialiśmy każdego z klientów.       |
| Lokalizacja pliku Windsurf                                                                                     | Niepotwierdzona, zobacz wyżej.                                                          |
| Konektory Claude Desktop i claude.ai                                                                           | Wymagają OAuth; obop nie można tam jeszcze dodać.                                       |
