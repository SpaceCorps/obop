---
title: "Yhdistä MCP:n kautta"
description: "Yhdistä Claude Desktop, Claude Code, Cursor, VS Code, Windsurf, Codex tai Gemini CLI obopiin Model Context Protocolilla: tarkat asetukset kullekin asiakkaalle, obop mcp stdio-yhteydellä."
lead: "obopin mukana tulee MCP-palvelin. Jokainen komento, jota tunniste saa käyttää, muuttuu MCP-työkaluksi. Tällä sivulla on asetukset yleisimmille asiakasohjelmille."
---

## Näin se toimii

`obop mcp` on MCP-palvelin, joka puhuu JSON-RPC:tä vakiosyötteen ja -tulosteen kautta. MCP-asiakas käynnistää sen aliprosessina. Palvelin:

- avaa instanssin tietokannan itse, joten sen on toimittava koneella, jolta tietokantaan (SQLite-tiedosto tai PostgreSQL-osoite) on yhteys;
- tunnistaa agentin agenttitunnisteesta;
- listaa yhden työkalun jokaista komentoa kohti, jota tunniste saa käyttää, nimellä `<model>__<command>`: `lead__search`, `lead__create`, `invoice__post`. Kaksoisalaviiva on samassa kohdassa kuin vinoviiva REST-polussa;
- vastaa `tools/call`-kutsuihin saman jakelijan kautta kuin REST, joten maskaus ja hyväksynnät ovat voimassa.

> **Isännöidyt instanssit.** Etä- eli HTTP-pohjaista MCP-siirtoa ei ole vielä, ja `obop mcp` tarvitsee tietokannan. Työpöydän MCP-asiakasta ei siksi voi tällä hetkellä yhdistää isännöityyn instanssiin. Käytä isännöityihin instansseihin [REST-rajapintaa](/docs/agents/rest/). MCP HTTP:n yli OAuthilla on suunnitteilla.

## Ennen aloitusta

1. Tarvitset käynnissä olevan instanssin tietokannan ja `obop`-ohjelman koneella, jolla MCP-asiakas ajetaan (tai sen absoluuttisen polun).
2. Tarvitset [agenttitunnisteen](/docs/agents/tokens/), jossa on haluamasi rajaukset.
3. Tarvitset tietokannan osoitteen muodossa `OBOP_DATABASE_URL` tai `--db`. Oletus on `sqlite://obop.db` nykyisessä hakemistossa, joten anna absoluuttinen polku: `sqlite:///home/me/obop.db`.

Jokaisen asiakkaan ajama komento on:

```bash
obop mcp --db sqlite:///home/me/obop.db --token obop_...
```

Se lukee `OBOP_TOKEN`- ja `OBOP_DATABASE_URL`-muuttujat ympäristöstä, jos valitsimia ei ole annettu. Suosi ympäristöä: silloin tunniste ei näy prosessiluettelossa.

Kokeile ensin käsin. Tämä lähettää `initialize`- ja `tools/list`-pyynnön ja tulostaa vastaukset:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

Palvelin ilmoittaa protokollaversion `2025-06-18` ja tukee kutsuja `initialize`, `ping`, `tools/list` ja `tools/call`. Työkalujen syötteen skeemat ovat toistaiseksi vain `{"type":"object"}`, joten asiakas ei voi validoida argumentteja; argumentit ovat samaa JSONia kuin [REST-pyynnön runko](/docs/reference/rest-api/).

Alla olevissa esimerkeissä käytetään polkua `/usr/local/bin/obop` ja paikkamerkkinä toimivaa tunnistetta. Korvaa molemmat. Muodot tarkistettiin kunkin asiakkaan dokumentaatiosta 16.10.2026; kohdat, joita ei voitu varmistaa, on merkitty.

## Claude Desktop

Avaa _Settings_, sitten _Developer_ ja _Edit Config_. Tiedosto on macOS:ssa `~/Library/Application Support/Claude/claude_desktop_config.json` ja Windowsissa `%APPDATA%\Claude\claude_desktop_config.json`. Lisää:

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

Sulje Claude Desktop ja käynnistä se uudelleen. Claude pyytää hyväksyntääsi ennen työkalun käyttöä.

## Claude Code

Käytä komentoriviä. Kaikki merkkien `--` jälkeen on palvelimen komento:

```bash
claude mcp add --transport stdio \
  --env OBOP_TOKEN=obop_your_agent_token \
  --env OBOP_DATABASE_URL=sqlite:///home/me/obop.db \
  obop -- /usr/local/bin/obop mcp
```

Laita viimeisen `--env`-valitsimen ja palvelimen nimen väliin vähintään yksi muu valitsin (tässä `--transport stdio`). Lisää `--scope project`, jos haluat kirjoittaa jaetun `.mcp.json`-tiedoston, tai `--scope user`, jos haluat käyttää asetusta kaikissa projekteissa. Projektitiedosto näyttää tältä; muoto `${...}` lukee arvon ympäristöstäsi, joten tunnistetta ei viedä versionhallintaan:

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

Tarkista asetus komennolla `claude mcp get obop` tai Claude Codessa komennolla `/mcp`.

## Cursor

Luo tiedosto `.cursor/mcp.json` projektiin tai `~/.cursor/mcp.json` kaikkia projekteja varten:

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

Cursorin omissa esimerkeissä `type` on jätetty pois stdio-palvelimilta, mutta sen kenttätaulukossa se mainitaan. Muoto `${env:NAME}` lukee arvon ympäristöstä, jossa Cursor on käynnistetty.

## VS Code

Luo tiedosto `.vscode/mcp.json`. VS Coden ylätason avain on `servers`, ei `mcpServers`, ja se voi kysyä tunnisteen, jolloin sitä ei koskaan kirjoiteta levylle:

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

VS Coden ohjattu _MCP: Add Server_ -komento voi ehdottaa sen sijaan siirrettävää `.mcp.json`-tiedostoa, jossa on `mcpServers`-muoto; dokumentaation mukaan molemmat hyväksytään.

## Windsurf

Windsurfin dokumentaatio on siirtynyt Devin Desktop -sivustolle, ja siinä tiedosto on nimeltään `mcp_config.json`. Windsurfin nykyistä tiedostosijaintia ei voitu varmistaa. Windsurf on aiemmin käyttänyt polkua `~/.codeium/windsurf/mcp_config.json`; Devin Desktop -sivulla annetaan polut `~/.config/devin/mcp_config.json` (macOS ja Linux) ja `%APPDATA%\devin\mcp_config.json` (Windows). Tarkista sijainti omasta versiostasi asetuksista. Muoto on sama `mcpServers`-rakenne:

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

Lisää taulu tiedostoon `~/.codex/config.toml` (tai luotetun projektin tiedostoon `.codex/config.toml`) tai aja `codex mcp add`:

```toml
[mcp_servers.obop]
command = "/usr/local/bin/obop"
args = ["mcp"]
env_vars = ["OBOP_TOKEN"]

[mcp_servers.obop.env]
OBOP_DATABASE_URL = "sqlite:///home/me/obop.db"
```

`env_vars` välittää `OBOP_TOKEN`-muuttujan komentotulkistasi sen sijaan, että se kirjoitettaisiin tiedostoon.

## Gemini CLI

Lisää tiedostoon `~/.gemini/settings.json`:

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

Gemini CLI:n dokumentaation mukaan se korvattiin toisella työkalulla vuonna 2026, joten tarkista oman versiosi ohjeet sieltä. Emme ole ajaneet obopia sen kanssa.

## Mikä tahansa muu MCP-asiakas

obop on tavallinen stdio-palvelin. Asiakkaan tiedosto voi näyttää miltä tahansa, mutta se tarvitsee `command`-arvon (`obop`), `args`-arvon (`["mcp"]`) ja kaksi ympäristömuuttujaa. Jos asiakas tukee vain etäpalvelimia, se ei voi vielä yhdistää.

## Mitä agentti näkee

Pyydä yhdistämisen jälkeen agenttia listaamaan työkalunsa. Näkyviin pitäisi tulla vain ne komennot, jotka sen rajaukset sallivat, ja hyväksyntää vaativien perässä on teksti "(agents propose; a human approves)". Kokeile ensin lukua, esimerkiksi _"List the three newest leads"_, ja katso sitten [Hyväksyntäkierros](/docs/agents/approvals/).

Jos asiakas ei näytä työkaluja tai näyttää virheen, siirry kohtaan [Vianmääritys](/docs/agents/troubleshooting/).

## Varmennettu ja varmentamatta

| Kohta                                                                                    | Tila                                                                             |
| ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `obop mcp` -kättely, `tools/list`, `tools/call`, hyväksynnän tulos                       | Ajettu oikeaa paikallista instanssia vasten.                                     |
| Claude Desktopin, Claude Coden, Cursorin, VS Coden, Codexin ja Gemini CLI:n asetusmuodot | Otettu kunkin asiakkaan dokumentaatiosta. Emme käynnistäneet jokaista asiakasta. |
| Windsurfin tiedostosijainti                                                              | Varmistamatta, katso yllä.                                                       |
